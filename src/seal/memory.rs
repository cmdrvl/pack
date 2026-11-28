use std::collections::HashSet;

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::detect::detect_member_type;
use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::manifest::{is_safe_member_path, Manifest, Member};
use crate::tar;

/// Seal named in-memory members into the same deterministic tar representation used by
/// `pack archive export`.
///
/// `created` is explicit by design: this API never consults a clock or SOURCE_DATE_EPOCH. The
/// input names are pack-relative paths, and lock matching is intentionally not part of sealing.
pub fn seal_to_tar_bytes(
    members: &[(&str, &[u8])],
    created: &str,
    note: Option<&str>,
) -> Result<Vec<u8>, Box<RefusalEnvelope>> {
    if members.is_empty() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::Empty,
            None,
            None,
        )));
    }

    let mut ordered = members.to_vec();
    ordered.sort_by(|left, right| left.0.cmp(right.0));

    let mut seen = HashSet::new();
    let mut manifest_members = Vec::with_capacity(ordered.len());
    for (path, bytes) in &ordered {
        if !is_safe_member_path(path) || path.split('/').any(|component| component == ".") {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!("Unsafe member path: {path}")),
                Some(json!({"path": path})),
            )));
        }
        if *path == "manifest.json" || !seen.insert(*path) {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::Duplicate,
                Some("Resolved member path collision".to_string()),
                Some(json!({"path": path})),
            )));
        }

        let detected = detect_member_type(bytes, path);
        manifest_members.push(Member {
            path: (*path).to_string(),
            bytes_hash: sha256_bytes(bytes),
            member_type: detected.member_type,
            artifact_version: detected.artifact_version,
            profile_frozen: detected.profile_frozen,
            profile_sha256: detected.profile_sha256,
            column_registry_hash: detected.column_registry_hash,
        });
    }

    let mut manifest = Manifest::new(
        created.to_string(),
        note.map(str::to_owned),
        None,
        env!("CARGO_PKG_VERSION").to_string(),
        manifest_members,
    );
    manifest.finalize();
    let manifest_bytes = manifest.to_canonical_bytes();

    let mut entries = Vec::with_capacity(ordered.len() + 1);
    entries.push(("manifest.json", manifest_bytes.as_slice()));
    entries.extend(ordered.iter().map(|(path, bytes)| (*path, *bytes)));
    tar::encode(&entries)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}
