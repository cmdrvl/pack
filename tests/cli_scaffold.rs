use serde_json::Value;
use std::process::Command;
use tiny_http::{Header, Method, Response, Server, StatusCode};

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

fn spawn_server(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", server.server_addr());
    let handle = std::thread::spawn(move || {
        let mut request = server.recv().unwrap();
        let method = request.method().clone();
        let url = request.url().to_string();
        let mut request_body = String::new();
        request
            .as_reader()
            .read_to_string(&mut request_body)
            .unwrap();
        let response = Response::from_string(body)
            .with_status_code(StatusCode(status))
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
        request.respond(response).unwrap();
        format!("{method:?} {url}\n{request_body}")
    });
    (base_url, handle)
}

#[test]
fn version_flag_exits_0() {
    let output = pack_cmd().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("pack "));
}

#[test]
fn help_flag_exits_0() {
    let output = pack_cmd().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("seal"));
    assert!(stdout.contains("verify"));
    assert!(stdout.contains("inspect"));
    assert!(stdout.contains("archive"));
    assert!(stdout.contains("witness"));
}

#[test]
fn describe_short_circuits_before_validation() {
    // --describe should exit 0 even without a subcommand
    let output = pack_cmd().arg("--describe").output().unwrap();
    assert!(output.status.success());
}

#[test]
fn describe_matches_checked_in_operator_json() {
    let output = pack_cmd().arg("--describe").output().unwrap();
    assert!(output.status.success());
    let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected: Value = serde_json::from_str(include_str!("../operator.json")).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn describe_lists_seal_outcome_option() {
    let output = pack_cmd().arg("--describe").output().unwrap();
    assert!(output.status.success());
    let op: Value = serde_json::from_slice(&output.stdout).unwrap();
    let seal = op["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|subcommand| subcommand["name"] == "seal")
        .unwrap();
    let has_outcome = seal["options"]
        .as_array()
        .unwrap()
        .iter()
        .any(|option| option["flag"] == "--outcome");
    assert!(has_outcome);
}

#[test]
fn schema_short_circuits_before_validation() {
    // --schema should exit 0 even without a subcommand
    let output = pack_cmd().arg("--schema").output().unwrap();
    assert!(output.status.success());
}

#[test]
fn schema_includes_primary_outcome_tag() {
    let output = pack_cmd().arg("--schema").output().unwrap();
    assert!(output.status.success());
    let schema: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        schema["definitions"]["manifest"]["properties"]["primary_outcome_tag"]["pattern"],
        "^cmdrvl://.+$"
    );
}

#[test]
fn no_command_exits_2() {
    let output = pack_cmd().output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn seal_stub_exits_2() {
    let output = pack_cmd().args(["seal", "foo.json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn verify_stub_exits_2() {
    let output = pack_cmd().args(["verify", "some_dir"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn inspect_json_exits_0_without_integrity_claim() {
    let tmp = tempfile::tempdir().unwrap();
    let artifact = tmp.path().join("data.json");
    std::fs::write(&artifact, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let pack_dir = tmp.path().join("pack");

    let seal = pack_cmd()
        .args([
            "--no-witness",
            "seal",
            artifact.to_str().unwrap(),
            "--output",
            pack_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(seal.status.success());

    let output = pack_cmd()
        .args(["inspect", pack_dir.to_str().unwrap(), "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["version"], "pack.inspect.v0");
    assert_eq!(payload["integrity"]["verified"], false);
    assert_eq!(payload["schema_validation"]["eligible_count"], 1);
}

#[test]
fn diff_nonexistent_packs_exits_2() {
    let output = pack_cmd().args(["diff", "a", "b"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn push_requires_base_url_env() {
    let output = pack_cmd().args(["push", "some_dir"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_IO");
    assert!(payload["refusal"]["message"]
        .as_str()
        .unwrap()
        .contains("PACK_DATA_FABRIC_BASE_URL"));
}

#[test]
fn push_success_exits_0_with_deterministic_status_line() {
    let tmp = tempfile::tempdir().unwrap();
    let artifact = tmp.path().join("data.json");
    std::fs::write(&artifact, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let pack_dir = tmp.path().join("pack");

    let seal = pack_cmd()
        .args([
            "--no-witness",
            "seal",
            artifact.to_str().unwrap(),
            "--output",
            pack_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(seal.status.success());

    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(pack_dir.join("manifest.json")).unwrap())
            .unwrap();
    let pack_id = manifest["pack_id"].as_str().unwrap().to_string();

    let (base_url, handle) = spawn_server(200, r#"{"status":"stored"}"#);
    let output = pack_cmd()
        .env("PACK_DATA_FABRIC_BASE_URL", &base_url)
        .args(["--no-witness", "push", pack_dir.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("PUBLISHED {pack_id}\n")
    );

    let request = handle.join().unwrap();
    assert!(request.starts_with(&format!("{:?} /packs/{pack_id}", Method::Put)));
}

#[test]
fn pull_requires_base_url_env() {
    let output = pack_cmd()
        .args(["pull", "sha256:abc", "--out", "dest"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_IO");
    assert!(payload["refusal"]["message"]
        .as_str()
        .unwrap()
        .contains("PACK_DATA_FABRIC_BASE_URL"));
}

#[test]
fn archive_export_import_cli_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let artifact = tmp.path().join("data.json");
    std::fs::write(&artifact, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let pack_dir = tmp.path().join("pack");
    let archive = tmp.path().join("pack.tar");
    let imported = tmp.path().join("imported");

    let seal = pack_cmd()
        .args([
            "--no-witness",
            "seal",
            artifact.to_str().unwrap(),
            "--output",
            pack_dir.to_str().unwrap(),
            "--created",
            "2026-01-15T10:30:00Z",
        ])
        .output()
        .unwrap();
    assert!(
        seal.status.success(),
        "seal stdout={} stderr={}",
        String::from_utf8_lossy(&seal.stdout),
        String::from_utf8_lossy(&seal.stderr)
    );

    let export = pack_cmd()
        .args([
            "archive",
            "export",
            pack_dir.to_str().unwrap(),
            "--out",
            archive.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(export.status.code(), Some(0));
    let export_stdout = String::from_utf8_lossy(&export.stdout);
    assert!(export_stdout.starts_with("ARCHIVE_CREATED sha256:"));
    assert!(archive.exists());

    let import = pack_cmd()
        .args([
            "archive",
            "import",
            archive.to_str().unwrap(),
            "--out",
            imported.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(import.status.code(), Some(0));
    let import_stdout = String::from_utf8_lossy(&import.stdout);
    assert!(import_stdout.starts_with("ARCHIVE_IMPORTED sha256:"));

    let verify = pack_cmd()
        .args([
            "verify",
            imported.to_str().unwrap(),
            "--json",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(verify.status.code(), Some(0));
}

#[test]
fn witness_last_exits_0() {
    let output = pack_cmd().args(["witness", "last"]).output().unwrap();
    assert!(output.status.success());
}

#[test]
fn witness_query_exits_0() {
    let output = pack_cmd().args(["witness", "query"]).output().unwrap();
    assert!(output.status.success());
}

#[test]
fn witness_count_exits_0() {
    let output = pack_cmd().args(["witness", "count"]).output().unwrap();
    assert!(output.status.success());
}

#[test]
fn global_no_witness_flag_accepted() {
    // --no-witness is a valid global flag, should not error from clap
    let output = pack_cmd()
        .args(["--no-witness", "seal", "foo.json"])
        .output()
        .unwrap();
    // Still exits 2 (stub), but clap didn't reject the flag
    assert_eq!(output.status.code(), Some(2));
}
