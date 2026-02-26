use std::fs;
use std::path::Path;

use serde_json::json;

use crate::seal::manifest::Manifest;
use crate::verify::VerifyReport;

use super::compare::compare_manifests;

/// Execute `pack diff <A> <B>`.
///
/// Returns (output_string, exit_code).
pub fn execute_diff(a_dir: &Path, b_dir: &Path, json_output: bool) -> (String, u8) {
    let a_manifest = match read_manifest(a_dir, "A") {
        Ok(m) => m,
        Err(report) => {
            let output = if json_output {
                report.to_json()
            } else {
                report.to_human()
            };
            return (output, 2);
        }
    };

    let b_manifest = match read_manifest(b_dir, "B") {
        Ok(m) => m,
        Err(report) => {
            let output = if json_output {
                report.to_json()
            } else {
                report.to_human()
            };
            return (output, 2);
        }
    };

    let diff = compare_manifests(&a_manifest, &b_manifest);

    let exit_code = if diff.has_changes() { 1 } else { 0 };

    let output = if json_output {
        diff.to_json()
    } else {
        diff.to_human()
    };

    (output, exit_code)
}

fn read_manifest(pack_dir: &Path, label: &str) -> Result<Manifest, Box<VerifyReport>> {
    let manifest_path = pack_dir.join("manifest.json");

    let content = fs::read_to_string(&manifest_path).map_err(|e| {
        Box::new(VerifyReport::refusal(json!({
            "code": "E_BAD_PACK",
            "message": format!("Cannot read manifest.json from pack {label}: {e}"),
        })))
    })?;

    let manifest: Manifest = serde_json::from_str(&content).map_err(|e| {
        Box::new(VerifyReport::refusal(json!({
            "code": "E_BAD_PACK",
            "message": format!("Invalid manifest.json in pack {label}: {e}"),
        })))
    })?;

    if manifest.version != "pack.v0" {
        return Err(Box::new(VerifyReport::refusal(json!({
            "code": "E_BAD_PACK",
            "message": format!("Unsupported manifest version in pack {label}: {}", manifest.version),
        }))));
    }

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_pack(members: &[(&str, &str)], note: Option<&str>) -> TempDir {
        let tmp = TempDir::new().unwrap();
        let pack_dir = tmp.path();

        // Write member files
        for (path, content) in members {
            let file_path = pack_dir.join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&file_path, content).unwrap();
        }

        // Build manifest
        use crate::seal::manifest::{Manifest, Member};
        use sha2::{Digest, Sha256};

        let members_vec: Vec<Member> = members
            .iter()
            .map(|(path, content)| {
                let mut hasher = Sha256::new();
                hasher.update(content.as_bytes());
                Member {
                    path: path.to_string(),
                    bytes_hash: format!("sha256:{}", hex::encode(hasher.finalize())),
                    member_type: "other".to_string(),
                    artifact_version: None,
                }
            })
            .collect();

        let mut manifest = Manifest::new(
            "2026-01-15T00:00:00Z".to_string(),
            note.map(|s| s.to_string()),
            "0.1.0".to_string(),
            members_vec,
        );
        manifest.finalize();

        std::fs::write(
            pack_dir.join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();

        tmp
    }

    #[test]
    fn identical_packs_exit_0() {
        let a = create_pack(&[("data.json", "hello")], None);
        let b = create_pack(&[("data.json", "hello")], None);

        let (output, code) = execute_diff(a.path(), b.path(), false);
        assert_eq!(code, 0);
        assert!(output.contains("NO_CHANGES"));
    }

    #[test]
    fn different_packs_exit_1() {
        let a = create_pack(&[("data.json", "hello")], None);
        let b = create_pack(&[("data.json", "world")], None);

        let (output, code) = execute_diff(a.path(), b.path(), false);
        assert_eq!(code, 1);
        assert!(output.contains("CHANGES"));
        assert!(output.contains("~ data.json"));
    }

    #[test]
    fn missing_pack_dir_exit_2() {
        let tmp = TempDir::new().unwrap();
        let (_, code) = execute_diff(Path::new("/nonexistent"), tmp.path(), false);
        assert_eq!(code, 2);
    }

    #[test]
    fn json_output_parses() {
        let a = create_pack(&[("x.json", "aaa")], None);
        let b = create_pack(&[("x.json", "aaa"), ("y.json", "bbb")], None);

        let (output, code) = execute_diff(a.path(), b.path(), true);
        assert_eq!(code, 1);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(report["outcome"], "CHANGES");
        assert_eq!(report["added"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn added_and_removed_detected() {
        let a = create_pack(&[("old.json", "data")], None);
        let b = create_pack(&[("new.json", "data")], None);

        let (output, code) = execute_diff(a.path(), b.path(), true);
        assert_eq!(code, 1);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(report["added"].as_array().unwrap().len(), 1);
        assert_eq!(report["removed"].as_array().unwrap().len(), 1);
    }
}
