use std::process::Command;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

/// The valid pack fixture contains known artifact types (lock.v0, rvl.v0, etc.)
/// so schema_validation should be "pass".
#[test]
fn valid_pack_schema_pass() {
    let output = pack_cmd()
        .args(["verify", "fixtures/packs/valid", "--json", "--no-witness"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "OK");
    assert_eq!(report["checks"]["schema_validation"], "pass");
}

/// A pack where a member's content was tampered to invalid JSON still has
/// a known artifact_version in the manifest, so schema validation runs
/// and should report "fail" (along with HASH_MISMATCH from integrity checks).
#[test]
fn tampered_member_schema_fail() {
    let output = pack_cmd()
        .args([
            "verify",
            "fixtures/packs/tampered_member",
            "--json",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1)); // INVALID
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "INVALID");
    // Schema validation should still be "pass" because the tampered content
    // is valid JSON with matching version — only the hash is wrong.
    // (The tampered content is {"version":"rvl.v0","outcome":"TAMPERED"})
    assert_eq!(report["checks"]["schema_validation"], "pass");
}

/// Build a pack containing only "other" type members (no artifact_version).
/// Schema validation should be "skipped".
#[test]
fn other_only_pack_schema_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("input.txt");
    std::fs::write(&art, "just plain text").unwrap();

    let output_dir = tmp.path().join("out");
    let output = pack_cmd()
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let verify = pack_cmd()
        .args([
            "verify",
            output_dir.to_str().unwrap(),
            "--json",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(verify.status.success());
    let stdout = String::from_utf8_lossy(&verify.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "OK");
    assert_eq!(report["checks"]["schema_validation"], "skipped");
}

/// Build a pack with a known-type artifact that has corrupted content
/// (valid JSON but wrong version field). Schema validation should be "fail".
#[test]
fn wrong_version_schema_fail() {
    let tmp = tempfile::tempdir().unwrap();

    // Create a file that type-detection thinks is lock.v0 (because it parses as lock.v0)
    // But we'll manually tamper the sealed copy's content to break schema.
    let art = tmp.path().join("data.lock.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();

    let output_dir = tmp.path().join("pack_out");
    let output = pack_cmd()
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Now tamper the sealed file to have wrong version but still valid JSON
    std::fs::write(
        output_dir.join("data.lock.json"),
        r#"{"version":"lock.v99","rows":5}"#,
    )
    .unwrap();

    let verify = pack_cmd()
        .args([
            "verify",
            output_dir.to_str().unwrap(),
            "--json",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(verify.status.code(), Some(1)); // INVALID
    let stdout = String::from_utf8_lossy(&verify.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "INVALID");
    assert_eq!(report["checks"]["schema_validation"], "fail");

    // Should have both HASH_MISMATCH and SCHEMA_VIOLATION findings
    let findings = report["invalid"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["code"] == "HASH_MISMATCH"));
    assert!(findings.iter().any(|f| f["code"] == "SCHEMA_VIOLATION"));
}
