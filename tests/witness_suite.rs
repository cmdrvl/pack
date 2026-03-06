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

fn seal_temp_pack(
    root: &std::path::Path,
    artifact_name: &str,
    content: &str,
) -> std::path::PathBuf {
    let artifact = root.join(artifact_name);
    std::fs::write(&artifact, content).unwrap();
    let output_dir = root.join(format!("{artifact_name}.pack"));

    let output = pack_cmd()
        .args([
            "--no-witness",
            "seal",
            artifact.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "seal should succeed when creating diff fixtures: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    output_dir
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
    assert_eq!(record["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(record["tool"], "pack");
    assert_eq!(record["command"], "seal");
    assert_eq!(record["outcome"], "PACK_CREATED");
    assert_eq!(record["exit_code"], 0);
    assert!(record["pack_id"].as_str().unwrap().starts_with("sha256:"));
    assert!(record["id"].as_str().unwrap().starts_with("blake3:"));
    assert!(record["binary_hash"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));
    assert!(record["output_hash"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));
    assert!(record["ts"].is_string());
    assert!(record["prev"].is_null());
    assert_eq!(record["inputs"][0]["path"], art.to_str().unwrap());
    assert!(record["inputs"][0]["hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(record["params"]["member_count"], 1);
    assert_eq!(record["params"]["output"], out.to_str().unwrap());
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
    assert_eq!(record["exit_code"], 2);
    assert!(record["pack_id"].is_null());
    assert!(record["output_hash"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));
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
    assert_eq!(record["exit_code"], 0);
    assert!(record["output_hash"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));
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
    assert_eq!(record["exit_code"], 1);
}

/// Diff with changes records CHANGES witness.
#[test]
fn diff_changes_records_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");
    let pack_a = seal_temp_pack(tmp.path(), "a.json", r#"{"version":"lock.v0","rows":1}"#);
    let pack_b = seal_temp_pack(tmp.path(), "b.json", r#"{"version":"lock.v0","rows":2}"#);

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["diff", pack_a.to_str().unwrap(), pack_b.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));

    let content = std::fs::read_to_string(&ledger).unwrap();
    let record: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(record["command"], "diff");
    assert_eq!(record["outcome"], "CHANGES");
    assert_eq!(record["exit_code"], 1);
    assert!(record["pack_id"].is_null());
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
    assert_eq!(record["exit_code"], 2);
}

/// Refusal diff records REFUSAL witness.
#[test]
fn diff_refusal_records_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["diff", "/nonexistent/pack", "fixtures/packs/valid"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));

    let content = std::fs::read_to_string(&ledger).unwrap();
    let record: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(record["command"], "diff");
    assert_eq!(record["outcome"], "REFUSAL");
    assert_eq!(record["exit_code"], 2);
}

/// Witness records chain prev ids across sequential operations.
#[test]
fn witness_records_chain_prev_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0"}"#).unwrap();
    let out = tmp.path().join("pack_out");

    let seal = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(seal.status.success());

    let verify = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["verify", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(verify.status.success());

    let content = std::fs::read_to_string(&ledger).unwrap();
    let records: Vec<serde_json::Value> = content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert!(records[0]["id"].as_str().unwrap().starts_with("blake3:"));
    assert_eq!(records[1]["prev"], records[0]["id"]);
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

/// --no-witness on diff suppresses witness append.
#[test]
fn no_witness_flag_suppresses_diff_append() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");
    let pack_a = seal_temp_pack(tmp.path(), "left.json", r#"{"version":"lock.v0","rows":1}"#);
    let pack_b = seal_temp_pack(
        tmp.path(),
        "right.json",
        r#"{"version":"lock.v0","rows":2}"#,
    );

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args([
            "--no-witness",
            "diff",
            pack_a.to_str().unwrap(),
            pack_b.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
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

/// When witness path is unwritable, diff still reports CHANGES (exit 1) with warning.
#[test]
fn witness_failure_preserves_diff_domain_outcome() {
    let tmp = tempfile::tempdir().unwrap();
    let pack_a = seal_temp_pack(
        tmp.path(),
        "diff-a.json",
        r#"{"version":"lock.v0","rows":1}"#,
    );
    let pack_b = seal_temp_pack(
        tmp.path(),
        "diff-b.json",
        r#"{"version":"lock.v0","rows":2}"#,
    );

    let output = pack_cmd()
        .env("EPISTEMIC_WITNESS", "/dev/null/impossible/witness.jsonl")
        .args(["diff", pack_a.to_str().unwrap(), pack_b.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CHANGES"));

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

/// witness query supports the standard filter surface.
#[test]
fn witness_query_filters_with_synthetic_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let records = [
        r#"{"id":"blake3:1","tool":"pack","version":"0.2.0","command":"seal","inputs":[{"path":"a.json","hash":"sha256:aaa","bytes":1}],"params":{"command":"seal"},"outcome":"PACK_CREATED","exit_code":0,"output_hash":"blake3:o1","ts":"2026-01-15T10:00:00Z"}"#,
        r#"{"id":"blake3:2","tool":"pack","version":"0.2.0","command":"seal","inputs":[{"path":"b.json","hash":"sha256:bbb","bytes":1}],"params":{"command":"seal"},"outcome":"PACK_CREATED","exit_code":0,"output_hash":"blake3:o2","ts":"2026-01-15T10:05:00Z"}"#,
        r#"{"id":"blake3:3","tool":"pack","version":"0.2.0","command":"verify","outcome":"REFUSAL","exit_code":2,"output_hash":"blake3:o3","ts":"2026-01-15T10:10:00Z"}"#,
    ];
    std::fs::write(&ledger, records.join("\n") + "\n").unwrap();

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args([
            "witness",
            "query",
            "--since",
            "2026-01-15T10:01:00Z",
            "--until",
            "2026-01-15T10:06:00Z",
            "--outcome",
            "PACK_CREATED",
            "--input-hash",
            "sha256:bbb",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["inputs"][0]["hash"], "sha256:bbb");
}

/// witness count can target other tools in the shared ledger.
#[test]
fn witness_count_honors_tool_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tmp.path().join("witness.jsonl");

    let records = [
        r#"{"id":"blake3:1","tool":"pack","version":"0.2.0","command":"seal","outcome":"PACK_CREATED","exit_code":0,"output_hash":"blake3:o1","ts":"2026-01-15T10:00:00Z"}"#,
        r#"{"id":"blake3:2","tool":"hash","version":"0.2.0","outcome":"OK","exit_code":0,"output_hash":"blake3:o2","ts":"2026-01-15T10:01:00Z"}"#,
        r#"{"id":"blake3:3","tool":"hash","version":"0.2.0","outcome":"OK","exit_code":0,"output_hash":"blake3:o3","ts":"2026-01-15T10:02:00Z"}"#,
    ];
    std::fs::write(&ledger, records.join("\n") + "\n").unwrap();

    let output = pack_cmd_with_witness(ledger.to_str().unwrap())
        .args(["witness", "count", "--tool", "hash", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["count"], 2);
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
