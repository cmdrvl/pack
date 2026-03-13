use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::manifest::Manifest;
use crate::verify::run_checks;

use super::push::DATA_FABRIC_BASE_URL_ENV;
use super::transport::{refusal_for_transport, DataFabricTransport, TransportRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullResult {
    pub pack_id: String,
    pub out_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredPack {
    pack_id: String,
    manifest: Manifest,
    members: Vec<StoredMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredMember {
    path: String,
    bytes_hash: String,
    bytes_b64: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedPack {
    pack_id: String,
    manifest: Manifest,
    members: Vec<DecodedMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedMember {
    path: String,
    bytes: Vec<u8>,
}

pub fn execute_pull(pack_id: &str, out_dir: &Path) -> Result<PullResult, Box<RefusalEnvelope>> {
    let base_url = data_fabric_base_url_from_env(|key| std::env::var(key).ok())?;
    execute_pull_with_base_url(pack_id, out_dir, &base_url)
}

fn execute_pull_with_base_url(
    pack_id: &str,
    out_dir: &Path,
    base_url: &str,
) -> Result<PullResult, Box<RefusalEnvelope>> {
    let request = TransportRequest::get(pack_path(pack_id));
    let transport = DataFabricTransport::new(base_url);
    let stored: StoredPack = transport
        .send_json(&request)
        .map_err(|error| Box::new(refusal_for_transport("pull", &error)))?;
    let decoded = decode_stored_pack(pack_id, stored)?;
    materialize_pack(&decoded, out_dir)?;

    Ok(PullResult {
        pack_id: decoded.pack_id,
        out_dir: out_dir.to_path_buf(),
    })
}

fn data_fabric_base_url_from_env<F>(get_env: F) -> Result<String, Box<RefusalEnvelope>>
where
    F: FnOnce(&str) -> Option<String>,
{
    let Some(raw) = get_env(DATA_FABRIC_BASE_URL_ENV) else {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "pack pull requires {DATA_FABRIC_BASE_URL_ENV} to be set"
            )),
            Some(json!({
                "env": DATA_FABRIC_BASE_URL_ENV,
            })),
        )));
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "pack pull requires non-empty {DATA_FABRIC_BASE_URL_ENV}"
            )),
            Some(json!({
                "env": DATA_FABRIC_BASE_URL_ENV,
            })),
        )));
    }

    Ok(trimmed.to_string())
}

fn decode_stored_pack(
    requested_pack_id: &str,
    stored: StoredPack,
) -> Result<DecodedPack, Box<RefusalEnvelope>> {
    if stored.pack_id != requested_pack_id {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Fetched pack_id does not match request: expected {requested_pack_id}, got {}",
                stored.pack_id
            )),
            Some(json!({
                "requested_pack_id": requested_pack_id,
                "actual_pack_id": stored.pack_id,
            })),
        )));
    }

    if stored.manifest.version != "pack.v0" {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Fetched manifest has unsupported version: {}",
                stored.manifest.version
            )),
            Some(json!({
                "pack_id": stored.pack_id,
                "version": stored.manifest.version,
            })),
        )));
    }

    if stored.manifest.pack_id != stored.pack_id {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Fetched manifest pack_id does not match payload pack_id: {} vs {}",
                stored.manifest.pack_id, stored.pack_id
            )),
            Some(json!({
                "requested_pack_id": requested_pack_id,
                "payload_pack_id": stored.pack_id,
                "manifest_pack_id": stored.manifest.pack_id,
            })),
        )));
    }

    let mut manifest_members = HashMap::new();
    for member in &stored.manifest.members {
        manifest_members.insert(member.path.clone(), member.bytes_hash.clone());
    }

    let mut seen_paths = HashSet::new();
    let mut member_bytes = HashMap::new();
    for member in stored.members {
        if !seen_paths.insert(member.path.clone()) {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!(
                    "Fetched payload contains duplicate member path: {}",
                    member.path
                )),
                Some(json!({
                    "pack_id": requested_pack_id,
                    "path": member.path,
                })),
            )));
        }

        let Some(expected_hash) = manifest_members.get(&member.path) else {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!(
                    "Fetched payload contains undeclared member: {}",
                    member.path
                )),
                Some(json!({
                    "pack_id": requested_pack_id,
                    "path": member.path,
                })),
            )));
        };

        if &member.bytes_hash != expected_hash {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!(
                    "Fetched member hash does not match manifest for {}",
                    member.path
                )),
                Some(json!({
                    "pack_id": requested_pack_id,
                    "path": member.path,
                    "expected": expected_hash,
                    "actual": member.bytes_hash,
                })),
            )));
        }

        let bytes = STANDARD.decode(member.bytes_b64).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!(
                    "Fetched member payload is not valid base64 for {}: {error}",
                    member.path
                )),
                Some(json!({
                    "pack_id": requested_pack_id,
                    "path": member.path,
                })),
            ))
        })?;

        let actual_hash = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
        if &actual_hash != expected_hash {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!(
                    "Fetched member bytes do not match manifest hash for {}",
                    member.path
                )),
                Some(json!({
                    "pack_id": requested_pack_id,
                    "path": member.path,
                    "expected": expected_hash,
                    "actual": actual_hash,
                })),
            )));
        }

        member_bytes.insert(member.path, bytes);
    }

    for manifest_member in &stored.manifest.members {
        if !member_bytes.contains_key(&manifest_member.path) {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!(
                    "Fetched payload is missing member bytes for {}",
                    manifest_member.path
                )),
                Some(json!({
                    "pack_id": requested_pack_id,
                    "path": manifest_member.path,
                })),
            )));
        }
    }

    let members = stored
        .manifest
        .members
        .iter()
        .map(|member| DecodedMember {
            path: member.path.clone(),
            bytes: member_bytes
                .remove(&member.path)
                .expect("validated member bytes must exist"),
        })
        .collect();

    Ok(DecodedPack {
        pack_id: stored.pack_id,
        manifest: stored.manifest,
        members,
    })
}

fn materialize_pack(decoded: &DecodedPack, out_dir: &Path) -> Result<(), Box<RefusalEnvelope>> {
    if out_dir.exists() {
        let mut entries = fs::read_dir(out_dir).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Cannot inspect output directory {}: {error}",
                    out_dir.display()
                )),
                None,
            ))
        })?;
        if entries.next().is_some() {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Output directory already exists and is non-empty: {}",
                    out_dir.display()
                )),
                None,
            )));
        }
    }

    let staging_parent = out_dir.parent().unwrap_or_else(|| Path::new("."));
    if !staging_parent.exists() {
        fs::create_dir_all(staging_parent).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Cannot create output parent directory {}: {error}",
                    staging_parent.display()
                )),
                None,
            ))
        })?;
    }

    let staging_dir = tempfile::Builder::new()
        .prefix(".pack-pull-")
        .tempdir_in(staging_parent)
        .map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!("Cannot create staging directory: {error}")),
                None,
            ))
        })?;

    write_decoded_pack(decoded, staging_dir.path())?;

    let (checks, findings) = run_checks(&decoded.manifest, staging_dir.path());
    if !findings.is_empty() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Fetched pack failed integrity checks after materialization: {}",
                decoded.pack_id
            )),
            Some(json!({
                "pack_id": decoded.pack_id,
                "checks": checks,
                "invalid": findings,
            })),
        )));
    }

    if out_dir.exists() {
        copy_dir_recursive(staging_dir.path(), out_dir)?;
        return Ok(());
    }

    match fs::rename(staging_dir.path(), out_dir) {
        Ok(()) => {
            let _ = staging_dir.keep();
            Ok(())
        }
        Err(_) => copy_dir_recursive(staging_dir.path(), out_dir),
    }
}

fn write_decoded_pack(decoded: &DecodedPack, dest_dir: &Path) -> Result<(), Box<RefusalEnvelope>> {
    for member in &decoded.members {
        let member_path = dest_dir.join(&member.path);
        if let Some(parent) = member_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Box::new(RefusalEnvelope::new(
                    RefusalCode::Io,
                    Some(format!(
                        "Cannot create parent directory for {}: {error}",
                        member.path
                    )),
                    None,
                ))
            })?;
        }

        fs::write(&member_path, &member.bytes).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Cannot write fetched member {}: {error}",
                    member.path
                )),
                None,
            ))
        })?;
    }

    fs::write(
        dest_dir.join("manifest.json"),
        decoded.manifest.to_canonical_bytes(),
    )
    .map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot write manifest.json: {error}")),
            None,
        ))
    })?;

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Box<RefusalEnvelope>> {
    fs::create_dir_all(dst).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot create directory {}: {error}",
                dst.display()
            )),
            None,
        ))
    })?;

    for entry in fs::read_dir(src).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot read staging dir: {error}")),
            None,
        ))
    })? {
        let entry = entry.map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!("Cannot read staging entry: {error}")),
                None,
            ))
        })?;

        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|error| {
                Box::new(RefusalEnvelope::new(
                    RefusalCode::Io,
                    Some(format!(
                        "Cannot copy {} to {}: {error}",
                        src_path.display(),
                        dst_path.display()
                    )),
                    None,
                ))
            })?;
        }
    }

    Ok(())
}

fn pack_path(pack_id: &str) -> String {
    format!("/packs/{pack_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        thread::{self, JoinHandle},
    };

    use tiny_http::{Header, Method, Response, Server, StatusCode};

    use crate::seal::command::execute_seal;

    struct MockServer {
        base_url: String,
        requests: mpsc::Receiver<(Method, String)>,
        handle: JoinHandle<()>,
    }

    impl MockServer {
        fn finish(self) -> Vec<(Method, String)> {
            self.handle.join().unwrap();
            self.requests.try_iter().collect()
        }
    }

    fn create_stored_pack() -> (tempfile::TempDir, StoredPack, String) {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let nested_dir = src.path().join("nested");
        let file = nested_dir.join("report.json");
        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(&file, r#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE"}"#).unwrap();

        let pack_dir = out.path().join("pack");
        let result =
            execute_seal(&[nested_dir], Some(&pack_dir), Some("pull me".to_string())).unwrap();
        let manifest: Manifest =
            serde_json::from_str(&fs::read_to_string(pack_dir.join("manifest.json")).unwrap())
                .unwrap();
        let bytes = fs::read(pack_dir.join("nested").join("report.json")).unwrap();
        let member_hash = manifest.members[0].bytes_hash.clone();
        let stored = StoredPack {
            pack_id: result.pack_id.clone(),
            manifest,
            members: vec![StoredMember {
                path: "nested/report.json".to_string(),
                bytes_hash: member_hash,
                bytes_b64: STANDARD.encode(bytes),
            }],
        };

        (out, stored, result.pack_id)
    }

    fn spawn_server(status: u16, body: String) -> MockServer {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            let request = server.recv().unwrap();
            tx.send((request.method().clone(), request.url().to_string()))
                .unwrap();
            let response = Response::from_string(body)
                .with_status_code(StatusCode(status))
                .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
            request.respond(response).unwrap();
        });

        MockServer {
            base_url,
            requests: rx,
            handle,
        }
    }

    #[test]
    fn fetches_pack_and_materializes_output_dir() {
        let (_out, stored, pack_id) = create_stored_pack();
        let server = spawn_server(200, serde_json::to_string(&stored).unwrap());
        let temp = tempfile::tempdir().unwrap();
        let out_dir = temp.path().join("fetched");

        let result = execute_pull_with_base_url(&pack_id, &out_dir, &server.base_url).unwrap();

        assert_eq!(result.pack_id, pack_id);
        assert_eq!(result.out_dir, out_dir);
        assert_eq!(
            fs::read_to_string(result.out_dir.join("nested").join("report.json")).unwrap(),
            r#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE"}"#
        );
        let manifest: Manifest = serde_json::from_str(
            &fs::read_to_string(result.out_dir.join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.pack_id, pack_id);

        let requests = server.finish();
        assert_eq!(requests, vec![(Method::Get, format!("/packs/{pack_id}"))]);
    }

    #[test]
    fn missing_base_url_env_refuses() {
        let error = data_fabric_base_url_from_env(|_| None).unwrap_err();
        assert_eq!(error.refusal.code, "E_IO");
        assert!(error.refusal.message.contains("PACK_DATA_FABRIC_BASE_URL"));
    }

    #[test]
    fn not_found_server_failure_maps_to_io_refusal() {
        let (_out, _stored, pack_id) = create_stored_pack();
        let server = spawn_server(404, r#"{"error":"missing"}"#.to_string());
        let temp = tempfile::tempdir().unwrap();
        let out_dir = temp.path().join("fetched");

        let error = execute_pull_with_base_url(&pack_id, &out_dir, &server.base_url).unwrap_err();

        assert_eq!(error.refusal.code, "E_IO");
        assert!(error.refusal.message.contains("HTTP 404"));
        let _ = server.finish();
    }

    #[test]
    fn malformed_payload_refuses_with_bad_pack() {
        let (_out, mut stored, pack_id) = create_stored_pack();
        stored.members[0].bytes_hash = "sha256:deadbeef".to_string();
        let server = spawn_server(200, serde_json::to_string(&stored).unwrap());
        let temp = tempfile::tempdir().unwrap();
        let out_dir = temp.path().join("fetched");

        let error = execute_pull_with_base_url(&pack_id, &out_dir, &server.base_url).unwrap_err();

        assert_eq!(error.refusal.code, "E_BAD_PACK");
        assert!(error.refusal.message.contains("does not match manifest"));
        let _ = server.finish();
    }

    #[test]
    fn transport_failures_map_to_io_refusal() {
        let (_out, _stored, pack_id) = create_stored_pack();
        let temp = tempfile::tempdir().unwrap();
        let out_dir = temp.path().join("fetched");

        let error =
            execute_pull_with_base_url(&pack_id, &out_dir, "http://127.0.0.1:9").unwrap_err();

        assert_eq!(error.refusal.code, "E_IO");
        assert!(error.refusal.message.contains("transport failure"));
    }
}
