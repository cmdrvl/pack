use std::process::Command;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

/// Parse refusal envelope from pack stdout. Asserts exit code 2 and returns the parsed JSON.
fn assert_refusal(output: std::process::Output) -> serde_json::Value {
    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected exit 2, got {:?}",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("Failed to parse refusal JSON: {e}\nstdout: {stdout}"))
}

/// Validate the envelope shape: version, outcome, refusal.code, refusal.message.
fn assert_envelope_shape(envelope: &serde_json::Value, expected_code: &str) {
    assert_eq!(envelope["version"], "pack.v0");
    assert_eq!(envelope["outcome"], "REFUSAL");
    assert_eq!(envelope["refusal"]["code"], expected_code);
    assert!(
        envelope["refusal"]["message"].is_string(),
        "refusal.message should be a string"
    );
    // next_command should be present (null is acceptable)
    assert!(envelope["refusal"].get("next_command").is_some());
}

// ---------------------------------------------------------------------------
// Seal refusals
// ---------------------------------------------------------------------------

/// E_EMPTY: seal with no artifacts (via missing required arg, clap catches this).
/// We test via the library path since clap would reject zero args.
/// Instead, test with an artifact that doesn't exist to get E_IO.
#[test]
fn seal_nonexistent_artifact_e_io() {
    let output = pack_cmd()
        .args(["seal", "/nonexistent/artifact.json", "--no-witness"])
        .output()
        .unwrap();
    let envelope = assert_refusal(output);
    assert_envelope_shape(&envelope, "E_IO");
    assert!(envelope["refusal"]["message"]
        .as_str()
        .unwrap()
        .contains("nonexistent"));
}

/// E_DUPLICATE: seal with two files that produce the same member path.
#[test]
fn seal_duplicate_path_e_duplicate() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("data.json");
    std::fs::write(&file, r#"{"version":"lock.v0"}"#).unwrap();

    let output = pack_cmd()
        .args([
            "seal",
            file.to_str().unwrap(),
            file.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    let envelope = assert_refusal(output);
    assert_envelope_shape(&envelope, "E_DUPLICATE");

    // Detail should include the conflicting path
    let detail = &envelope["refusal"]["detail"];
    assert!(detail.is_object(), "detail should be an object");
    assert_eq!(detail["path"], "data.json");
}

/// E_IO: seal to a non-empty output directory.
#[test]
fn seal_non_empty_output_e_io() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("input.json");
    std::fs::write(&art, r#"{"version":"lock.v0"}"#).unwrap();

    let out = tmp.path().join("occupied");
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("existing.txt"), "data").unwrap();

    let output = pack_cmd()
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    let envelope = assert_refusal(output);
    assert_envelope_shape(&envelope, "E_IO");
    assert!(envelope["refusal"]["message"]
        .as_str()
        .unwrap()
        .contains("non-empty"));
}

// ---------------------------------------------------------------------------
// Verify refusals
// ---------------------------------------------------------------------------

/// E_BAD_PACK: verify on directory with no manifest.json.
#[test]
fn verify_no_manifest_e_bad_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let output = pack_cmd()
        .args([
            "verify",
            tmp.path().to_str().unwrap(),
            "--json",
            "--no-witness",
        ])
        .output()
        .unwrap();
    // verify uses its own report format, not refusal envelope
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "REFUSAL");
    assert_eq!(report["refusal"]["code"], "E_BAD_PACK");
}

/// E_BAD_PACK: verify on directory with malformed manifest.json.
#[test]
fn verify_malformed_manifest_e_bad_pack() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("manifest.json"), "{{{BROKEN").unwrap();

    let output = pack_cmd()
        .args([
            "verify",
            tmp.path().to_str().unwrap(),
            "--json",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "REFUSAL");
    assert_eq!(report["refusal"]["code"], "E_BAD_PACK");
    assert!(report["refusal"]["message"]
        .as_str()
        .unwrap()
        .contains("Invalid manifest.json"));
}

/// E_BAD_PACK: verify on directory with unsupported manifest version.
#[test]
fn verify_wrong_version_e_bad_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = serde_json::json!({
        "version": "pack.v99",
        "pack_id": "sha256:fake",
        "created": "2026-01-01T00:00:00Z",
        "tool_version": "0.1.0",
        "member_count": 0,
        "members": []
    });
    std::fs::write(
        tmp.path().join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();

    let output = pack_cmd()
        .args([
            "verify",
            tmp.path().to_str().unwrap(),
            "--json",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "REFUSAL");
    assert_eq!(report["refusal"]["code"], "E_BAD_PACK");
    assert!(report["refusal"]["message"]
        .as_str()
        .unwrap()
        .contains("Unsupported"));
}

/// E_BAD_PACK: verify on nonexistent directory.
#[test]
fn verify_nonexistent_dir_e_bad_pack() {
    let output = pack_cmd()
        .args(["verify", "/nonexistent/pack/dir", "--json", "--no-witness"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["outcome"], "REFUSAL");
    assert_eq!(report["refusal"]["code"], "E_BAD_PACK");
}

// ---------------------------------------------------------------------------
// Envelope stability
// ---------------------------------------------------------------------------

/// Seal refusal envelope fields are deterministic across invocations.
#[test]
fn seal_refusal_envelope_is_deterministic() {
    let run = || {
        let output = pack_cmd()
            .args(["seal", "/nonexistent/file.json", "--no-witness"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        (
            envelope["version"].clone(),
            envelope["outcome"].clone(),
            envelope["refusal"]["code"].clone(),
        )
    };

    let (v1, o1, c1) = run();
    let (v2, o2, c2) = run();
    assert_eq!(v1, v2);
    assert_eq!(o1, o2);
    assert_eq!(c1, c2);
}

/// Verify refusal human output starts with "pack verify: REFUSAL".
#[test]
fn verify_refusal_human_output() {
    let tmp = tempfile::tempdir().unwrap();
    let output = pack_cmd()
        .args(["verify", tmp.path().to_str().unwrap(), "--no-witness"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REFUSAL"));
}
