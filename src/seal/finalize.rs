use std::fs;
use std::path::Path;

use crate::detect::detect_member_type;
use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::copy::CopiedMember;
use crate::seal::manifest::{Manifest, Member};

/// Build the manifest from copied members, finalize pack_id, and write manifest.json.
///
/// Steps:
/// 1. For each copied member, read content to detect type and artifact version.
/// 2. Build members list sorted by path (already sorted from collect).
/// 3. Create manifest with `pack_id: ""`, finalize via self-hash.
/// 4. Write `manifest.json` into the staging directory.
pub fn finalize_manifest(
    copied: &[CopiedMember],
    staging_dir: &Path,
    created: String,
    note: Option<String>,
) -> Result<Manifest, Box<RefusalEnvelope>> {
    let tool_version = env!("CARGO_PKG_VERSION").to_string();

    let mut members = Vec::with_capacity(copied.len());
    for cm in copied {
        let file_path = staging_dir.join(&cm.member_path);
        let content = fs::read(&file_path).map_err(|e| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Cannot read copied member for type detection: {}: {e}",
                    cm.member_path
                )),
                None,
            ))
        })?;

        let detected = detect_member_type(&content, &cm.member_path);

        members.push(Member {
            path: cm.member_path.clone(),
            bytes_hash: cm.bytes_hash.clone(),
            member_type: detected.member_type,
            artifact_version: detected.artifact_version,
        });
    }

    let mut manifest = Manifest::new(created, note, tool_version, members);
    manifest.finalize();

    // Write manifest.json
    let manifest_bytes = manifest.to_canonical_bytes();
    let manifest_path = staging_dir.join("manifest.json");
    fs::write(&manifest_path, &manifest_bytes).map_err(|e| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot write manifest.json: {e}")),
            None,
        ))
    })?;

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_staging() -> (TempDir, Vec<CopiedMember>) {
        let staging = TempDir::new().unwrap();

        // Write a known-type file
        let lock_content = br#"{"version": "lock.v0", "rows": 100}"#;
        fs::write(staging.path().join("nov.lock.json"), lock_content).unwrap();

        // Write an unknown-type file
        fs::write(staging.path().join("notes.txt"), b"hello").unwrap();

        let copied = vec![
            CopiedMember {
                member_path: "nov.lock.json".to_string(),
                bytes_hash: "sha256:aaa".to_string(),
                size: lock_content.len() as u64,
            },
            CopiedMember {
                member_path: "notes.txt".to_string(),
                bytes_hash: "sha256:bbb".to_string(),
                size: 5,
            },
        ];
        (staging, copied)
    }

    #[test]
    fn builds_manifest_with_finalized_pack_id() {
        let (staging, copied) = setup_staging();
        let manifest = finalize_manifest(
            &copied,
            staging.path(),
            "2026-01-15T10:30:00Z".to_string(),
            None,
        )
        .unwrap();

        assert!(manifest.pack_id.starts_with("sha256:"));
        assert_eq!(manifest.member_count, 2);
        assert_eq!(manifest.version, "pack.v0");
    }

    #[test]
    fn detects_member_types_correctly() {
        let (staging, copied) = setup_staging();
        let manifest = finalize_manifest(
            &copied,
            staging.path(),
            "2026-01-15T10:30:00Z".to_string(),
            None,
        )
        .unwrap();

        let lock_member = manifest.members.iter().find(|m| m.path == "nov.lock.json");
        assert_eq!(lock_member.unwrap().member_type, "lockfile");
        assert_eq!(
            lock_member.unwrap().artifact_version.as_deref(),
            Some("lock.v0")
        );

        let txt_member = manifest.members.iter().find(|m| m.path == "notes.txt");
        assert_eq!(txt_member.unwrap().member_type, "other");
        assert_eq!(txt_member.unwrap().artifact_version, None);
    }

    #[test]
    fn writes_manifest_json_to_staging() {
        let (staging, copied) = setup_staging();
        finalize_manifest(
            &copied,
            staging.path(),
            "2026-01-15T10:30:00Z".to_string(),
            None,
        )
        .unwrap();

        let manifest_path = staging.path().join("manifest.json");
        assert!(manifest_path.exists());

        // Should be parseable back
        let content = fs::read_to_string(&manifest_path).unwrap();
        let parsed: Manifest = serde_json::from_str(&content).unwrap();
        assert!(parsed.pack_id.starts_with("sha256:"));
        assert_eq!(parsed.member_count, 2);
    }

    #[test]
    fn pack_id_is_self_verifiable() {
        let (staging, copied) = setup_staging();
        let manifest = finalize_manifest(
            &copied,
            staging.path(),
            "2026-01-15T10:30:00Z".to_string(),
            None,
        )
        .unwrap();

        // Recompute should match
        let recomputed = manifest.recompute_pack_id();
        assert_eq!(manifest.pack_id, recomputed);
    }

    #[test]
    fn note_included_in_manifest() {
        let (staging, copied) = setup_staging();
        let manifest = finalize_manifest(
            &copied,
            staging.path(),
            "2026-01-15T10:30:00Z".to_string(),
            Some("Q4 reconciliation".to_string()),
        )
        .unwrap();

        assert_eq!(manifest.note.as_deref(), Some("Q4 reconciliation"));
    }

    #[test]
    fn member_count_matches_members_len() {
        let (staging, copied) = setup_staging();
        let manifest = finalize_manifest(
            &copied,
            staging.path(),
            "2026-01-15T10:30:00Z".to_string(),
            None,
        )
        .unwrap();

        assert_eq!(manifest.member_count, manifest.members.len());
    }
}
