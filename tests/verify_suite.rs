use std::process::Command;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

fn verify_json(pack_dir: &str) -> (serde_json::Value, i32) {
    let output = pack_cmd()
        .args(["verify", pack_dir, "--json", "--no-witness"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("Failed to parse verify JSON for {pack_dir}: {e}\nstdout: {stdout}")
    });
    (report, code)
}

// ---------------------------------------------------------------------------
// OK outcome (exit 0)
// ---------------------------------------------------------------------------

/// Valid pack fixture verifies OK with all checks passing.
#[test]
fn valid_pack_verifies_ok() {
    let (report, code) = verify_json("fixtures/packs/valid");
    assert_eq!(code, 0);
    assert_eq!(report["outcome"], "OK");
    assert_eq!(report["version"], "pack.verify.v0");
    assert!(report["pack_id"].as_str().unwrap().starts_with("sha256:"));
    assert!(report["invalid"].as_array().unwrap().is_empty());

    // All integrity checks must pass
    let checks = &report["checks"];
    assert_eq!(checks["manifest_parse"], true);
    assert_eq!(checks["member_count"], true);
    assert_eq!(checks["member_paths"], true);
    assert_eq!(checks["extra_members"], true);
    assert_eq!(checks["member_hashes"], true);
    assert_eq!(checks["pack_id"], true);
    assert_eq!(checks["schema_validation"], "pass");
}

/// Human-readable output format for valid pack.
#[test]
fn valid_pack_human_output() {
    let output = pack_cmd()
        .args(["verify", "fixtures/packs/valid", "--no-witness"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pack verify: OK"));
    assert!(stdout.contains("pack_id: sha256:"));
}

// ---------------------------------------------------------------------------
// INVALID outcomes (exit 1) — committed fixtures
// ---------------------------------------------------------------------------

/// Missing member produces MISSING_MEMBER finding.
#[test]
fn missing_member_is_invalid() {
    let (report, code) = verify_json("fixtures/packs/missing_member");
    assert_eq!(code, 1);
    assert_eq!(report["outcome"], "INVALID");

    let findings = report["invalid"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["code"] == "MISSING_MEMBER"));

    let f = findings
        .iter()
        .find(|f| f["code"] == "MISSING_MEMBER")
        .unwrap();
    assert_eq!(f["path"], "rvl.report.json");

    // member_hashes should fail
    assert_eq!(report["checks"]["member_hashes"], false);
}

/// Tampered member content produces HASH_MISMATCH finding.
#[test]
fn tampered_member_is_invalid() {
    let (report, code) = verify_json("fixtures/packs/tampered_member");
    assert_eq!(code, 1);
    assert_eq!(report["outcome"], "INVALID");

    let findings = report["invalid"].as_array().unwrap();
    let mismatch = findings
        .iter()
        .find(|f| f["code"] == "HASH_MISMATCH")
        .unwrap();
    assert_eq!(mismatch["path"], "rvl.report.json");
    assert!(mismatch["expected"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(mismatch["actual"].as_str().unwrap().starts_with("sha256:"));
    assert_ne!(mismatch["expected"], mismatch["actual"]);
}

/// Tampered manifest (changed note) produces PACK_ID_MISMATCH.
#[test]
fn tampered_manifest_is_invalid() {
    let (report, code) = verify_json("fixtures/packs/tampered_manifest");
    assert_eq!(code, 1);
    assert_eq!(report["outcome"], "INVALID");

    let findings = report["invalid"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["code"] == "PACK_ID_MISMATCH"));

    // pack_id check should fail
    assert_eq!(report["checks"]["pack_id"], false);
}

/// Extra undeclared file produces EXTRA_MEMBER finding.
#[test]
fn extra_member_is_invalid() {
    let (report, code) = verify_json("fixtures/packs/extra_member");
    assert_eq!(code, 1);
    assert_eq!(report["outcome"], "INVALID");

    let findings = report["invalid"].as_array().unwrap();
    let extra = findings
        .iter()
        .find(|f| f["code"] == "EXTRA_MEMBER")
        .unwrap();
    assert_eq!(extra["path"], "undeclared.txt");

    // extra_members check should fail
    assert_eq!(report["checks"]["extra_members"], false);
}

// ---------------------------------------------------------------------------
// INVALID outcomes (exit 1) — constructed at test time
// ---------------------------------------------------------------------------

/// Manifest with unsafe member path (path traversal) produces UNSAFE_MEMBER_PATH.
#[test]
fn unsafe_member_path_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let pack_dir = tmp.path().join("bad_pack");
    std::fs::create_dir(&pack_dir).unwrap();

    // Write a member file
    std::fs::write(pack_dir.join("data.json"), r#"{"version":"lock.v0"}"#).unwrap();

    // Craft a manifest with an unsafe path
    let manifest = serde_json::json!({
        "version": "pack.v0",
        "pack_id": "sha256:fake",
        "created": "2026-01-15T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 1,
        "members": [{
            "path": "../escape.json",
            "bytes_hash": "sha256:0000",
            "type": "other"
        }]
    });
    std::fs::write(
        pack_dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    let (report, code) = verify_json(pack_dir.to_str().unwrap());
    assert_eq!(code, 1);
    let findings = report["invalid"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["code"] == "UNSAFE_MEMBER_PATH"));
}

/// Manifest with duplicate member paths produces DUPLICATE_MEMBER_PATH.
#[test]
fn duplicate_member_path_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let pack_dir = tmp.path().join("dup_pack");
    std::fs::create_dir(&pack_dir).unwrap();

    std::fs::write(pack_dir.join("data.json"), r#"{"version":"lock.v0"}"#).unwrap();

    let manifest = serde_json::json!({
        "version": "pack.v0",
        "pack_id": "sha256:fake",
        "created": "2026-01-15T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 2,
        "members": [
            {"path": "data.json", "bytes_hash": "sha256:0000", "type": "lockfile"},
            {"path": "data.json", "bytes_hash": "sha256:0000", "type": "lockfile"}
        ]
    });
    std::fs::write(
        pack_dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    let (report, code) = verify_json(pack_dir.to_str().unwrap());
    assert_eq!(code, 1);
    let findings = report["invalid"].as_array().unwrap();
    assert!(findings
        .iter()
        .any(|f| f["code"] == "DUPLICATE_MEMBER_PATH"));
}

/// Manifest with reserved "manifest.json" member path produces RESERVED_MEMBER_PATH.
#[test]
fn reserved_member_path_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let pack_dir = tmp.path().join("reserved_pack");
    std::fs::create_dir(&pack_dir).unwrap();

    let manifest = serde_json::json!({
        "version": "pack.v0",
        "pack_id": "sha256:fake",
        "created": "2026-01-15T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 1,
        "members": [{
            "path": "manifest.json",
            "bytes_hash": "sha256:0000",
            "type": "other"
        }]
    });
    std::fs::write(
        pack_dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    let (report, code) = verify_json(pack_dir.to_str().unwrap());
    assert_eq!(code, 1);
    let findings = report["invalid"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["code"] == "RESERVED_MEMBER_PATH"));
}

/// member_count mismatch between field and array length.
#[test]
fn member_count_mismatch_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let pack_dir = tmp.path().join("count_pack");
    std::fs::create_dir(&pack_dir).unwrap();

    std::fs::write(pack_dir.join("data.json"), r#"{"version":"lock.v0"}"#).unwrap();

    let manifest = serde_json::json!({
        "version": "pack.v0",
        "pack_id": "sha256:fake",
        "created": "2026-01-15T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 5,  // wrong — only 1 member
        "members": [{
            "path": "data.json",
            "bytes_hash": "sha256:0000",
            "type": "lockfile"
        }]
    });
    std::fs::write(
        pack_dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    let (report, code) = verify_json(pack_dir.to_str().unwrap());
    assert_eq!(code, 1);
    let findings = report["invalid"].as_array().unwrap();
    assert!(findings
        .iter()
        .any(|f| f["code"] == "MEMBER_COUNT_MISMATCH"));
    assert_eq!(report["checks"]["member_count"], false);
}

/// Symlink as member produces NON_REGULAR_MEMBER finding.
#[cfg(unix)]
#[test]
fn symlink_member_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let pack_dir = tmp.path().join("sym_pack");
    std::fs::create_dir(&pack_dir).unwrap();

    // Create a real file and a symlink
    let real_file = tmp.path().join("real.json");
    std::fs::write(&real_file, r#"{"version":"lock.v0"}"#).unwrap();
    std::os::unix::fs::symlink(&real_file, pack_dir.join("link.json")).unwrap();

    // Compute hash of the real file for the manifest
    use sha2::{Digest, Sha256};
    let content = std::fs::read(&real_file).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let hash = format!("sha256:{}", hex::encode(hasher.finalize()));

    let manifest = serde_json::json!({
        "version": "pack.v0",
        "pack_id": "sha256:fake",
        "created": "2026-01-15T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 1,
        "members": [{
            "path": "link.json",
            "bytes_hash": hash,
            "type": "lockfile"
        }]
    });
    std::fs::write(
        pack_dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    let (report, code) = verify_json(pack_dir.to_str().unwrap());
    assert_eq!(code, 1);
    let findings = report["invalid"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["code"] == "NON_REGULAR_MEMBER"));
}

// ---------------------------------------------------------------------------
// REFUSAL outcomes (exit 2)
// ---------------------------------------------------------------------------

/// Nonexistent pack directory produces REFUSAL.
#[test]
fn nonexistent_pack_dir_is_refusal() {
    let (report, code) = verify_json("/nonexistent/pack/dir");
    assert_eq!(code, 2);
    assert_eq!(report["outcome"], "REFUSAL");
    assert!(report["refusal"].is_object());
}

/// Pack directory with malformed manifest.json produces REFUSAL.
#[test]
fn malformed_manifest_is_refusal() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("manifest.json"), "NOT VALID JSON {{{").unwrap();

    let (report, code) = verify_json(tmp.path().to_str().unwrap());
    assert_eq!(code, 2);
    assert_eq!(report["outcome"], "REFUSAL");
    assert_eq!(report["refusal"]["code"], "E_BAD_PACK");
}

/// Pack directory with no manifest.json produces REFUSAL.
#[test]
fn missing_manifest_is_refusal() {
    let tmp = tempfile::tempdir().unwrap();
    // Empty directory, no manifest.json

    let (report, code) = verify_json(tmp.path().to_str().unwrap());
    assert_eq!(code, 2);
    assert_eq!(report["outcome"], "REFUSAL");
}

/// Manifest with unsupported version produces REFUSAL.
#[test]
fn unsupported_manifest_version_is_refusal() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = serde_json::json!({
        "version": "pack.v99",
        "pack_id": "sha256:fake",
        "created": "2026-01-15T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 0,
        "members": []
    });
    std::fs::write(
        tmp.path().join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    let (report, code) = verify_json(tmp.path().to_str().unwrap());
    assert_eq!(code, 2);
    assert_eq!(report["outcome"], "REFUSAL");
}

// ---------------------------------------------------------------------------
// JSON output structure
// ---------------------------------------------------------------------------

/// JSON output includes all required top-level fields.
#[test]
fn json_output_has_all_fields() {
    let (report, _) = verify_json("fixtures/packs/valid");

    // Top-level fields
    assert!(report.get("version").is_some());
    assert!(report.get("outcome").is_some());
    assert!(report.get("pack_id").is_some());
    assert!(report.get("checks").is_some());
    assert!(report.get("invalid").is_some());

    // Checks fields
    let checks = &report["checks"];
    assert!(checks.get("manifest_parse").is_some());
    assert!(checks.get("member_count").is_some());
    assert!(checks.get("member_paths").is_some());
    assert!(checks.get("extra_members").is_some());
    assert!(checks.get("member_hashes").is_some());
    assert!(checks.get("pack_id").is_some());
    assert!(checks.get("schema_validation").is_some());
}

/// Multiple findings from different categories appear in the same report.
#[test]
fn multiple_findings_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let pack_dir = tmp.path().join("multi_bad");
    std::fs::create_dir(&pack_dir).unwrap();

    // Manifest declares two members but files don't exist
    let manifest = serde_json::json!({
        "version": "pack.v0",
        "pack_id": "sha256:fake",
        "created": "2026-01-15T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 2,
        "members": [
            {"path": "a.json", "bytes_hash": "sha256:aaaa", "type": "other"},
            {"path": "b.json", "bytes_hash": "sha256:bbbb", "type": "other"}
        ]
    });
    std::fs::write(
        pack_dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    // Add an extra undeclared file
    std::fs::write(pack_dir.join("extra.txt"), "rogue").unwrap();

    let (report, code) = verify_json(pack_dir.to_str().unwrap());
    assert_eq!(code, 1);

    let findings = report["invalid"].as_array().unwrap();
    let codes: Vec<&str> = findings
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();

    // Should have MISSING_MEMBER for both a.json and b.json, plus EXTRA_MEMBER
    assert!(codes.iter().filter(|&&c| c == "MISSING_MEMBER").count() >= 2);
    assert!(codes.contains(&"EXTRA_MEMBER"));
    assert!(codes.contains(&"PACK_ID_MISMATCH"));
}
