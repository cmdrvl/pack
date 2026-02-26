use std::fs;
use std::path::Path;

use serde_json::json;

use crate::seal::manifest::Manifest;

use super::checks::run_checks;
use super::report::{VerifyOutcome, VerifyReport};

/// Execute `pack verify` on a pack directory.
///
/// Returns (report, exit_code).
pub fn execute_verify(pack_dir: &Path, json_output: bool) -> (String, u8) {
    // Step 1: Read manifest.json
    let manifest_path = pack_dir.join("manifest.json");

    let manifest_content = match fs::read_to_string(&manifest_path) {
        Ok(content) => content,
        Err(e) => {
            let report = VerifyReport::refusal(json!({
                "code": "E_BAD_PACK",
                "message": format!("Cannot read manifest.json: {e}"),
            }));
            let output = if json_output {
                report.to_json()
            } else {
                report.to_human()
            };
            return (output, 2);
        }
    };

    // Step 2: Parse manifest
    let manifest: Manifest = match serde_json::from_str(&manifest_content) {
        Ok(m) => m,
        Err(e) => {
            let report = VerifyReport::refusal(json!({
                "code": "E_BAD_PACK",
                "message": format!("Invalid manifest.json: {e}"),
            }));
            let output = if json_output {
                report.to_json()
            } else {
                report.to_human()
            };
            return (output, 2);
        }
    };

    // Step 3: Validate pack.v0
    if manifest.version != "pack.v0" {
        let report = VerifyReport::refusal(json!({
            "code": "E_BAD_PACK",
            "message": format!("Unsupported manifest version: {}", manifest.version),
        }));
        let output = if json_output {
            report.to_json()
        } else {
            report.to_human()
        };
        return (output, 2);
    }

    // Step 4: Run integrity checks
    let (checks, findings) = run_checks(&manifest, pack_dir);

    let report = if findings.is_empty() {
        VerifyReport::ok(manifest.pack_id.clone(), checks)
    } else {
        VerifyReport::invalid(Some(manifest.pack_id.clone()), checks, findings)
    };

    let exit_code = match report.outcome {
        VerifyOutcome::OK => 0,
        VerifyOutcome::INVALID => 1,
        VerifyOutcome::REFUSAL => 2,
    };

    let output = if json_output {
        report.to_json()
    } else {
        report.to_human()
    };

    (output, exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::command::execute_seal;
    use std::fs;
    use tempfile::TempDir;

    fn create_valid_pack() -> (TempDir, String) {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let file = src.path().join("data.lock.json");
        fs::write(&file, r#"{"version":"lock.v0","rows":5}"#).unwrap();

        let result = execute_seal(&[file], Some(&out.path().join("p")), None).unwrap();
        (out, result.pack_id)
    }

    #[test]
    fn valid_pack_verifies_ok() {
        let (out, _pack_id) = create_valid_pack();
        let (output, code) = execute_verify(&out.path().join("p"), false);
        assert_eq!(code, 0);
        assert!(output.contains("OK"));
    }

    #[test]
    fn valid_pack_json_output() {
        let (out, pack_id) = create_valid_pack();
        let (output, code) = execute_verify(&out.path().join("p"), true);
        assert_eq!(code, 0);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(report["outcome"], "OK");
        assert_eq!(report["pack_id"], pack_id);
        assert_eq!(report["version"], "pack.verify.v0");
    }

    #[test]
    fn missing_manifest_is_refusal() {
        let tmp = TempDir::new().unwrap();
        let (output, code) = execute_verify(tmp.path(), true);
        assert_eq!(code, 2);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(report["outcome"], "REFUSAL");
    }

    #[test]
    fn tampered_member_is_invalid() {
        let (out, _) = create_valid_pack();
        let pack_path = out.path().join("p");
        // Tamper with the member
        fs::write(pack_path.join("data.lock.json"), "TAMPERED").unwrap();

        let (output, code) = execute_verify(&pack_path, true);
        assert_eq!(code, 1);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(report["outcome"], "INVALID");
        let findings = report["invalid"].as_array().unwrap();
        assert!(findings.iter().any(|f| f["code"] == "HASH_MISMATCH"));
    }

    #[test]
    fn extra_file_is_invalid() {
        let (out, _) = create_valid_pack();
        let pack_path = out.path().join("p");
        fs::write(pack_path.join("extra.txt"), "sneaky").unwrap();

        let (output, code) = execute_verify(&pack_path, true);
        assert_eq!(code, 1);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(report["invalid"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "EXTRA_MEMBER"));
    }

    #[test]
    fn missing_member_is_invalid() {
        let (out, _) = create_valid_pack();
        let pack_path = out.path().join("p");
        fs::remove_file(pack_path.join("data.lock.json")).unwrap();

        let (output, code) = execute_verify(&pack_path, true);
        assert_eq!(code, 1);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(report["invalid"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "MISSING_MEMBER"));
    }

    #[test]
    fn tampered_manifest_pack_id_is_invalid() {
        let (out, _) = create_valid_pack();
        let pack_path = out.path().join("p");
        let manifest_path = pack_path.join("manifest.json");
        let content = fs::read_to_string(&manifest_path).unwrap();
        let tampered = content.replace("sha256:", "sha256:0000");
        fs::write(&manifest_path, tampered).unwrap();

        let (output, code) = execute_verify(&pack_path, true);
        assert_eq!(code, 1);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(report["invalid"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "PACK_ID_MISMATCH" || f["code"] == "HASH_MISMATCH"));
    }

    #[test]
    fn invalid_json_manifest_is_refusal() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("manifest.json"), "NOT JSON").unwrap();

        let (_, code) = execute_verify(tmp.path(), true);
        assert_eq!(code, 2);
    }
}
