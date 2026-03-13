use std::fs;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::manifest::Manifest;
use crate::verify::run_checks;

use super::transport::{refusal_for_transport, DataFabricTransport, TransportRequest};

pub const DATA_FABRIC_BASE_URL_ENV: &str = "PACK_DATA_FABRIC_BASE_URL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushResult {
    pub pack_id: String,
}

pub fn execute_push(pack_dir: &Path) -> Result<PushResult, Box<RefusalEnvelope>> {
    let base_url = data_fabric_base_url_from_env(|key| std::env::var(key).ok())?;
    execute_push_with_base_url(pack_dir, &base_url)
}

fn execute_push_with_base_url(
    pack_dir: &Path,
    base_url: &str,
) -> Result<PushResult, Box<RefusalEnvelope>> {
    let manifest = load_and_validate_manifest(pack_dir)?;
    let payload = build_publish_payload(pack_dir, &manifest)?;
    let request = TransportRequest::put(pack_path(&manifest.pack_id), payload);
    let transport = DataFabricTransport::new(base_url);

    transport
        .send(&request)
        .map_err(|error| Box::new(refusal_for_transport("push", &error)))?;

    Ok(PushResult {
        pack_id: manifest.pack_id,
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
                "pack push requires {DATA_FABRIC_BASE_URL_ENV} to be set"
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
                "pack push requires non-empty {DATA_FABRIC_BASE_URL_ENV}"
            )),
            Some(json!({
                "env": DATA_FABRIC_BASE_URL_ENV,
            })),
        )));
    }

    Ok(trimmed.to_string())
}

fn load_and_validate_manifest(pack_dir: &Path) -> Result<Manifest, Box<RefusalEnvelope>> {
    let manifest_path = pack_dir.join("manifest.json");
    let manifest_content = fs::read_to_string(&manifest_path).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Cannot read manifest.json: {error}")),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
            })),
        ))
    })?;

    let manifest: Manifest = serde_json::from_str(&manifest_content).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Invalid manifest.json: {error}")),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
            })),
        ))
    })?;

    if manifest.version != "pack.v0" {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Unsupported manifest version: {}",
                manifest.version
            )),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
                "version": manifest.version,
            })),
        )));
    }

    let (checks, findings) = run_checks(&manifest, pack_dir);
    if !findings.is_empty() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Pack directory failed integrity checks for publish: {}",
                manifest.pack_id
            )),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "pack_id": manifest.pack_id,
                "checks": checks,
                "invalid": findings,
            })),
        )));
    }

    Ok(manifest)
}

fn build_publish_payload(
    pack_dir: &Path,
    manifest: &Manifest,
) -> Result<Value, Box<RefusalEnvelope>> {
    let mut members = Vec::with_capacity(manifest.members.len());
    for member in &manifest.members {
        let member_path = pack_dir.join(&member.path);
        let bytes = fs::read(&member_path).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Cannot read member for publish {}: {error}",
                    member.path
                )),
                Some(json!({
                    "pack_dir": pack_dir.display().to_string(),
                    "path": member.path,
                })),
            ))
        })?;

        members.push(json!({
            "path": member.path,
            "bytes_hash": member.bytes_hash,
            "bytes_b64": STANDARD.encode(bytes),
        }));
    }

    Ok(json!({
        "pack_id": manifest.pack_id,
        "manifest": manifest,
        "members": members,
    }))
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
        requests: mpsc::Receiver<(Method, String, String)>,
        handle: JoinHandle<()>,
    }

    impl MockServer {
        fn finish(self) -> Vec<(Method, String, String)> {
            self.handle.join().unwrap();
            self.requests.try_iter().collect()
        }
    }

    fn create_valid_pack() -> (tempfile::TempDir, std::path::PathBuf, String) {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let file = src.path().join("report.json");
        fs::write(&file, r#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE"}"#).unwrap();

        let pack_dir = out.path().join("pack");
        let result =
            execute_seal(&[file], Some(&pack_dir), Some("publish me".to_string())).unwrap();
        (out, pack_dir, result.pack_id)
    }

    fn spawn_server(status: u16, body: &'static str) -> MockServer {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            let mut request = server.recv().unwrap();
            let mut request_body = String::new();
            request
                .as_reader()
                .read_to_string(&mut request_body)
                .unwrap();
            tx.send((
                request.method().clone(),
                request.url().to_string(),
                request_body,
            ))
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
    fn publish_puts_manifest_and_members_by_pack_id() {
        let (_out, pack_dir, pack_id) = create_valid_pack();
        let server = spawn_server(200, r#"{"status":"stored"}"#);

        let result = execute_push_with_base_url(&pack_dir, &server.base_url).unwrap();

        assert_eq!(result.pack_id, pack_id);

        let requests = server.finish();
        assert_eq!(requests.len(), 1);
        let (method, path, body) = &requests[0];
        assert_eq!(*method, Method::Put);
        assert_eq!(path, &format!("/packs/{pack_id}"));

        let payload: Value = serde_json::from_str(body).unwrap();
        assert_eq!(payload["pack_id"], pack_id);
        assert_eq!(payload["manifest"]["pack_id"], pack_id);
        let members = payload["members"].as_array().unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0]["path"], "report.json");
        assert_eq!(
            members[0]["bytes_b64"],
            STANDARD.encode(r#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE"}"#)
        );
    }

    #[test]
    fn invalid_pack_refuses_before_network_publish() {
        let (_out, pack_dir, _pack_id) = create_valid_pack();
        fs::write(pack_dir.join("report.json"), "tampered").unwrap();

        let error = execute_push_with_base_url(&pack_dir, "http://127.0.0.1:9").unwrap_err();

        assert_eq!(error.refusal.code, "E_BAD_PACK");
        assert!(error.refusal.message.contains("failed integrity checks"));
    }

    #[test]
    fn missing_base_url_env_refuses() {
        let error = data_fabric_base_url_from_env(|_| None).unwrap_err();
        assert_eq!(error.refusal.code, "E_IO");
        assert!(error.refusal.message.contains("PACK_DATA_FABRIC_BASE_URL"));
    }

    #[test]
    fn transport_failures_map_to_io_refusal() {
        let (_out, pack_dir, _pack_id) = create_valid_pack();

        let error = execute_push_with_base_url(&pack_dir, "http://127.0.0.1:9").unwrap_err();

        assert_eq!(error.refusal.code, "E_IO");
        assert!(error.refusal.message.contains("transport failure"));
    }
}
