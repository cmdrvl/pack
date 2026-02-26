use std::process::Command;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

/// Create a pack command with EPISTEMIC_WITNESS pointing to a temp file.
fn pack_cmd_with_witness(ledger_path: &str) -> Command {
    let mut cmd = pack_cmd();
    cmd.env("EPISTEMIC_WITNESS", ledger_path);
    cmd
}

// ---------------------------------------------------------------------------
// Witness append: seal records PACK_CREATED
// ---------------------------------------------------------------------------

/// Successful seal records PACK_CREATED witness with pack_id.
#[test]
fn seal_records_pack_created_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let out = tmp.path().join("pack_out");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let content = std::fs::read_to_string(&ledger).unwrap();
    let lines: Vec<&str> = content.trim().lines().collect();
    assert_eq!(lines.len(), 1);

    let record: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(record["version"], "witness.v0");
    assert_eq!(record["tool"], "pack");
    assert_eq!(record["command"], "seal");
    assert_eq!(record["outcome"], "PACK_CREATED");
    assert!(record["pack_id"].as_str().unwrap().starts_with("sha256:"));
    assert!(record["timestamp"].is_string());
}

/// Failed seal records REFUSAL witness.
#[test]
fn seal_failure_records_refusal_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["seal", "/nonexistent/file.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));

    let content = std::fs::read_to_string(&ledger).unwrap();
    let record: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(record["command"], "seal");
    assert_eq!(record["outcome"], "REFUSAL");
    assert!(record["pack_id"].is_null());
}

// ---------------------------------------------------------------------------
// Witness append: verify records OK/INVALID/REFUSAL
// ---------------------------------------------------------------------------

/// Successful verify records OK witness.
#[test]
fn verify_ok_records_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["verify", "fixtures/packs/valid"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let content = std::fs::read_to_string(&ledger).unwrap();
    let record: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(record["command"], "verify");
    assert_eq!(record["outcome"], "OK");
}

/// Invalid verify records INVALID witness.
#[test]
fn verify_invalid_records_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["verify", "fixtures/packs/missing_member"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));

    let content = std::fs::read_to_string(&ledger).unwrap();
    let record: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(record["command"], "verify");
    assert_eq!(record["outcome"], "INVALID");
}

/// Refusal verify records REFUSAL witness.
#[test]
fn verify_refusal_records_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["verify", "/nonexistent/dir"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));

    let content = std::fs::read_to_string(&ledger).unwrap();
    let record: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(record["command"], "verify");
    assert_eq!(record["outcome"], "REFUSAL");
}

// ---------------------------------------------------------------------------
// --no-witness suppresses append
// ---------------------------------------------------------------------------

/// --no-witness on seal suppresses witness append.
#[test]
fn no_witness_flag_suppresses_seal_append() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0"}"#).unwrap();
    let out = tmp.path().join("pack_out");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args([
            "--no-witness",
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!ledger.exists(), "Witness ledger should not be created");
}

/// --no-witness on verify suppresses witness append.
#[test]
fn no_witness_flag_suppresses_verify_append() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["--no-witness", "verify", "fixtures/packs/valid"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!ledger.exists(), "Witness ledger should not be created");
}

// ---------------------------------------------------------------------------
// Domain outcome preservation: witness failure doesn't change exit code
// ---------------------------------------------------------------------------

/// When witness path is unwritable, seal still succeeds (exit 0) with warning.
#[test]
fn witness_failure_preserves_seal_domain_outcome() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0"}"#).unwrap();
    let out = tmp.path().join("pack_out");

    // Point witness to a path inside a nonexistent directory hierarchy
    // that we'll make unwritable. Use /dev/null/foo which will fail.
    let output = pack_cmd()
        .env("EPISTEMIC_WITNESS", "/dev/null/impossible/witness.jsonl")
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    // Domain outcome: seal succeeded
    assert!(
        output.status.success(),
        "Seal should succeed even if witness fails"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PACK_CREATED"));

    // Warning should appear on stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("witness append warning"),
        "stderr should warn about witness failure: {stderr}"
    );
}

/// When witness path is unwritable, verify still reports OK (exit 0) with warning.
#[test]
fn witness_failure_preserves_verify_domain_outcome() {
    let output = pack_cmd()
        .env("EPISTEMIC_WITNESS", "/dev/null/impossible/witness.jsonl")
        .args(["verify", "fixtures/packs/valid"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Verify should succeed even if witness fails"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OK"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("witness append warning"),
        "stderr should warn about witness failure: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Witness query subcommands with synthetic ledger
// ---------------------------------------------------------------------------

/// witness query with synthetic ledger returns records in human format.
#[test]
fn witness_query_human_with_synthetic_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    // Write synthetic records
    let records = [
        r#"{"version":"witness.v0","tool":"pack","command":"seal","outcome":"PACK_CREATED","pack_id":"sha256:aaa","timestamp":"2026-01-15T10:00:00.000Z"}"#,
        r#"{"version":"witness.v0","tool":"pack","command":"verify","outcome":"OK","pack_id":null,"timestamp":"2026-01-15T10:01:00.000Z"}"#,
    ];
    std::fs::write(&ledger, records.join("\n") + "\n").unwrap();

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "query"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("seal"));
    assert!(stdout.contains("PACK_CREATED"));
    assert!(stdout.contains("verify"));
    assert!(stdout.contains("OK"));
}

/// witness query --json returns JSON array.
#[test]
fn witness_query_json_with_synthetic_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let records = [
        r#"{"version":"witness.v0","tool":"pack","command":"seal","outcome":"PACK_CREATED","pack_id":"sha256:aaa","timestamp":"2026-01-15T10:00:00.000Z"}"#,
        r#"{"version":"witness.v0","tool":"pack","command":"verify","outcome":"OK","pack_id":null,"timestamp":"2026-01-15T10:01:00.000Z"}"#,
    ];
    std::fs::write(&ledger, records.join("\n") + "\n").unwrap();

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "query", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0]["command"], "seal");
    assert_eq!(parsed[1]["command"], "verify");
}

/// witness last returns the most recent record.
#[test]
fn witness_last_with_synthetic_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let records = [
        r#"{"version":"witness.v0","tool":"pack","command":"seal","outcome":"PACK_CREATED","pack_id":"sha256:aaa","timestamp":"2026-01-15T10:00:00.000Z"}"#,
        r#"{"version":"witness.v0","tool":"pack","command":"verify","outcome":"INVALID","pack_id":null,"timestamp":"2026-01-15T10:01:00.000Z"}"#,
    ];
    std::fs::write(&ledger, records.join("\n") + "\n").unwrap();

    // Human format
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "last"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("verify"));
    assert!(stdout.contains("INVALID"));

    // JSON format
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "last", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["command"], "verify");
    assert_eq!(parsed["outcome"], "INVALID");
}

/// witness count returns correct count.
#[test]
fn witness_count_with_synthetic_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let records = [
        r#"{"version":"witness.v0","tool":"pack","command":"seal","outcome":"PACK_CREATED","pack_id":null,"timestamp":"2026-01-15T10:00:00.000Z"}"#,
        r#"{"version":"witness.v0","tool":"pack","command":"verify","outcome":"OK","pack_id":null,"timestamp":"2026-01-15T10:01:00.000Z"}"#,
        r#"{"version":"witness.v0","tool":"pack","command":"seal","outcome":"REFUSAL","pack_id":null,"timestamp":"2026-01-15T10:02:00.000Z"}"#,
    ];
    std::fs::write(&ledger, records.join("\n") + "\n").unwrap();

    // Human format
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "count"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3 witness record(s)"));

    // JSON format
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "count", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["count"], 3);
}

/// witness query on empty ledger returns appropriate output.
#[test]
fn witness_query_empty_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    // Human: no records message
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "query"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No witness records found"));

    // JSON: empty array
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "query", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "[]");
}

/// Witness accumulates across multiple operations.
#[test]
fn witness_accumulates_across_operations() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    // Seal an artifact
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0"}"#).unwrap();
    let out = tmp.path().join("pack_out");
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify the sealed pack
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["verify", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Count should be 2
    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "count", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["count"], 2);
}
