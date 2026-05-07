use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde_json::json;

use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::collect::collect_artifacts;
use crate::seal::collision::check_collisions;
use crate::seal::copy::copy_and_hash;
use crate::seal::finalize::finalize_manifest;
use crate::staging::{
    create_staging_dir, ensure_output_available, ensure_parent_exists, output_parent,
    promote_staging,
};
use crate::witness::WitnessInput;

/// Execute the full `pack seal` flow.
///
/// Steps:
/// 1. Collect and normalize artifact inputs
/// 2. Check for path collisions
/// 3. Prepare staging directory
/// 4. Copy members and compute hashes
/// 5. Build and finalize manifest with pack_id
/// 6. Atomically promote staging dir to final output
pub fn execute_seal(
    artifacts: &[PathBuf],
    output: Option<&Path>,
    note: Option<String>,
    created: Option<&str>,
) -> Result<SealResult, Box<RefusalEnvelope>> {
    // 1. Collect
    let candidates = collect_artifacts(artifacts)?;

    // 2. Collision check
    check_collisions(&candidates)?;

    // 3. Resolve reproducible creation timestamp, then stage pack bytes.
    let created = created_timestamp(created)?;

    // Stage beside the final output so promotion can use atomic rename.
    if let Some(dir) = output {
        ensure_output_available(dir)?;
    }
    let staging_parent = match output {
        Some(dir) => output_parent(dir),
        None => PathBuf::from("pack"),
    };
    ensure_parent_exists(&staging_parent)?;
    let staging_dir = create_staging_dir(&staging_parent, ".pack-seal-")?;

    // 4. Copy and hash
    let copied = copy_and_hash(&candidates, staging_dir.path())?;

    // 5. Finalize manifest
    let manifest = finalize_manifest(&copied, staging_dir.path(), created, note)?;

    // 6. Determine final output path and atomically promote
    let final_dir = match output {
        Some(dir) => dir.to_path_buf(),
        None => PathBuf::from("pack").join(&manifest.pack_id),
    };

    promote_staging(staging_dir, &final_dir)?;

    Ok(SealResult {
        pack_id: manifest.pack_id.clone(),
        output_dir: final_dir,
        created: manifest.created,
        member_count: manifest.member_count,
        witness_inputs: candidates
            .iter()
            .zip(copied.iter())
            .map(|(candidate, copied_member)| WitnessInput {
                path: candidate.source.display().to_string(),
                hash: Some(copied_member.bytes_hash.clone()),
                bytes: Some(copied_member.size),
            })
            .collect(),
    })
}

/// Result of a successful seal operation.
#[derive(Debug)]
pub struct SealResult {
    pub pack_id: String,
    pub output_dir: PathBuf,
    pub created: String,
    pub member_count: usize,
    pub witness_inputs: Vec<WitnessInput>,
}

fn created_timestamp(cli_created: Option<&str>) -> Result<String, Box<RefusalEnvelope>> {
    let source_date_epoch = if cli_created.is_some() {
        None
    } else {
        std::env::var("SOURCE_DATE_EPOCH").ok()
    };
    resolve_created_timestamp(cli_created, source_date_epoch.as_deref())
}

fn resolve_created_timestamp(
    cli_created: Option<&str>,
    source_date_epoch: Option<&str>,
) -> Result<String, Box<RefusalEnvelope>> {
    if let Some(value) = cli_created {
        return parse_created_flag(value);
    }

    if let Some(value) = source_date_epoch {
        return parse_source_date_epoch(value);
    }

    Ok(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn parse_created_flag(value: &str) -> Result<String, Box<RefusalEnvelope>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .map_err(|e| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Invalid --created timestamp: expected RFC3339 timestamp: {e}"
                )),
                Some(json!({
                    "flag": "--created",
                    "value": value,
                    "expected": "RFC3339 timestamp"
                })),
            ))
        })
}

fn parse_source_date_epoch(value: &str) -> Result<String, Box<RefusalEnvelope>> {
    let seconds = value.parse::<i64>().map_err(|e| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Invalid SOURCE_DATE_EPOCH: expected Unix seconds: {e}"
            )),
            Some(json!({
                "env": "SOURCE_DATE_EPOCH",
                "value": value,
                "expected": "Unix seconds"
            })),
        ))
    })?;

    Utc.timestamp_opt(seconds, 0)
        .single()
        .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
        .ok_or_else(|| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some("Invalid SOURCE_DATE_EPOCH: Unix seconds out of range".to_string()),
                Some(json!({
                    "env": "SOURCE_DATE_EPOCH",
                    "value": value,
                    "expected": "Unix seconds"
                })),
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_artifacts(tmp: &TempDir) -> Vec<PathBuf> {
        let lock = tmp.path().join("nov.lock.json");
        fs::write(&lock, r#"{"version": "lock.v0", "rows": 10}"#).unwrap();

        let report = tmp.path().join("rvl.report.json");
        fs::write(
            &report,
            r#"{"version": "rvl.v0", "outcome": "NO_REAL_CHANGE"}"#,
        )
        .unwrap();

        vec![lock, report]
    }

    #[test]
    fn seal_creates_pack_directory() {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let artifacts = create_test_artifacts(&src);
        let output_dir = out.path().join("my_pack");

        let result = execute_seal(&artifacts, Some(&output_dir), None, None).unwrap();

        assert!(result.pack_id.starts_with("sha256:"));
        assert_eq!(result.member_count, 2);
        assert!(result.output_dir.join("manifest.json").exists());
        assert!(result.output_dir.join("nov.lock.json").exists());
        assert!(result.output_dir.join("rvl.report.json").exists());
    }

    #[test]
    fn seal_manifest_is_valid_json() {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let artifacts = create_test_artifacts(&src);
        let output_dir = out.path().join("pack_out");

        let result = execute_seal(&artifacts, Some(&output_dir), None, None).unwrap();
        let manifest_content = fs::read_to_string(result.output_dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();

        assert_eq!(manifest["version"], "pack.v0");
        assert!(manifest["pack_id"].as_str().unwrap().starts_with("sha256:"));
        assert_eq!(manifest["member_count"], 2);
    }

    #[test]
    fn seal_with_note() {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let artifacts = create_test_artifacts(&src);
        let output_dir = out.path().join("noted_pack");

        let result = execute_seal(
            &artifacts,
            Some(&output_dir),
            Some("Q4 recon".to_string()),
            None,
        )
        .unwrap();
        let manifest_content = fs::read_to_string(result.output_dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
        assert_eq!(manifest["note"], "Q4 recon");
    }

    #[test]
    fn seal_refuses_non_empty_output_dir() {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let artifacts = create_test_artifacts(&src);
        let output_dir = out.path().join("occupied");

        fs::create_dir(&output_dir).unwrap();
        fs::write(output_dir.join("existing.txt"), "data").unwrap();

        let err = execute_seal(&artifacts, Some(&output_dir), None, None).unwrap_err();
        assert_eq!(err.refusal.code, "E_IO");
        assert!(err.refusal.message.contains("non-empty"));
    }

    #[test]
    fn seal_promotes_into_existing_empty_output_dir() {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let artifacts = create_test_artifacts(&src);
        let output_dir = out.path().join("empty");
        fs::create_dir(&output_dir).unwrap();

        let result = execute_seal(&artifacts, Some(&output_dir), None, None).unwrap();

        assert_eq!(result.output_dir, output_dir);
        assert!(result.output_dir.join("manifest.json").exists());
        assert!(result.output_dir.join("nov.lock.json").exists());
        assert!(result.output_dir.join("rvl.report.json").exists());
    }

    #[test]
    fn seal_empty_artifacts_refuses() {
        let err = execute_seal(&[], None, None, None).unwrap_err();
        assert_eq!(err.refusal.code, "E_EMPTY");
    }

    #[test]
    fn seal_member_bytes_match_source() {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();

        let content = r#"{"version": "lock.v0", "test": true}"#;
        let file = src.path().join("data.lock.json");
        fs::write(&file, content).unwrap();

        let output_dir = out.path().join("byte_check");
        let result = execute_seal(&[file], Some(&output_dir), None, None).unwrap();

        let copied = fs::read_to_string(result.output_dir.join("data.lock.json")).unwrap();
        assert_eq!(copied, content);
    }

    #[test]
    fn created_flag_normalizes_rfc3339_to_utc() {
        let created =
            resolve_created_timestamp(Some("2026-01-15T05:30:00-05:00"), Some("42")).unwrap();
        assert_eq!(created, "2026-01-15T10:30:00Z");
    }

    #[test]
    fn source_date_epoch_used_when_created_flag_absent() {
        let created = resolve_created_timestamp(None, Some("42")).unwrap();
        assert_eq!(created, "1970-01-01T00:00:42Z");
    }

    #[test]
    fn invalid_created_flag_refuses() {
        let err = resolve_created_timestamp(Some("not-rfc3339"), None).unwrap_err();
        assert_eq!(err.refusal.code, "E_IO");
        assert_eq!(
            err.refusal.detail.as_ref().unwrap()["flag"],
            serde_json::json!("--created")
        );
    }

    #[test]
    fn invalid_source_date_epoch_refuses() {
        let err = resolve_created_timestamp(None, Some("not-epoch")).unwrap_err();
        assert_eq!(err.refusal.code, "E_IO");
        assert_eq!(
            err.refusal.detail.as_ref().unwrap()["env"],
            serde_json::json!("SOURCE_DATE_EPOCH")
        );
    }
}
