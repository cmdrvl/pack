use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::collect::collect_artifacts;
use crate::seal::collision::check_collisions;
use crate::seal::copy::copy_and_hash;
use crate::seal::finalize::finalize_manifest;

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
) -> Result<SealResult, Box<RefusalEnvelope>> {
    // 1. Collect
    let candidates = collect_artifacts(artifacts)?;

    // 2. Collision check
    check_collisions(&candidates)?;

    // 3. Staging dir (in parent of final output or system temp)
    let created = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Create staging in system temp
    let staging_dir = tempfile::tempdir().map_err(|e| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot create staging directory: {e}")),
            None,
        ))
    })?;

    // 4. Copy and hash
    let copied = copy_and_hash(&candidates, staging_dir.path())?;

    // 5. Finalize manifest
    let manifest = finalize_manifest(&copied, staging_dir.path(), created, note)?;

    // 6. Determine final output path and atomically promote
    let final_dir = match output {
        Some(dir) => dir.to_path_buf(),
        None => PathBuf::from("pack").join(&manifest.pack_id),
    };

    // Refuse if target exists and is non-empty
    if final_dir.exists() {
        let is_empty = fs::read_dir(&final_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Output directory already exists and is non-empty: {}",
                    final_dir.display()
                )),
                None,
            )));
        }
    }

    // Create parent of final_dir if needed
    if let Some(parent) = final_dir.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                Box::new(RefusalEnvelope::new(
                    RefusalCode::Io,
                    Some(format!("Cannot create output parent directory: {}", e)),
                    None,
                ))
            })?;
        }
    }

    // Atomic rename from staging to final
    // Note: rename may fail across filesystems; in that case, fall back to copy
    if fs::rename(staging_dir.path(), &final_dir).is_err() {
        // Fallback: copy tree
        copy_dir_recursive(staging_dir.path(), &final_dir)?;
    }

    // Prevent tempdir cleanup from failing (dir was moved)
    // into_path() consumes the TempDir without trying to remove it
    let _ = staging_dir.keep();

    Ok(SealResult {
        pack_id: manifest.pack_id.clone(),
        output_dir: final_dir,
        member_count: manifest.member_count,
    })
}

/// Result of a successful seal operation.
#[derive(Debug)]
pub struct SealResult {
    pub pack_id: String,
    pub output_dir: PathBuf,
    pub member_count: usize,
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Box<RefusalEnvelope>> {
    fs::create_dir_all(dst).map_err(|e| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot create directory {}: {e}", dst.display())),
            None,
        ))
    })?;

    for entry in fs::read_dir(src).map_err(|e| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot read staging dir: {e}")),
            None,
        ))
    })? {
        let entry = entry.map_err(|e| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!("Cannot read staging entry: {e}")),
                None,
            ))
        })?;

        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                Box::new(RefusalEnvelope::new(
                    RefusalCode::Io,
                    Some(format!(
                        "Cannot copy {} to {}: {e}",
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

        let result = execute_seal(&artifacts, Some(&output_dir), None).unwrap();

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

        let result = execute_seal(&artifacts, Some(&output_dir), None).unwrap();
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

        let result =
            execute_seal(&artifacts, Some(&output_dir), Some("Q4 recon".to_string())).unwrap();
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

        let err = execute_seal(&artifacts, Some(&output_dir), None).unwrap_err();
        assert_eq!(err.refusal.code, "E_IO");
        assert!(err.refusal.message.contains("non-empty"));
    }

    #[test]
    fn seal_empty_artifacts_refuses() {
        let err = execute_seal(&[], None, None).unwrap_err();
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
        let result = execute_seal(&[file], Some(&output_dir), None).unwrap();

        let copied = fs::read_to_string(result.output_dir.join("data.lock.json")).unwrap();
        assert_eq!(copied, content);
    }
}
