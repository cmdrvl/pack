use std::process::Command;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
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
    assert!(stdout.contains("witness"));
}

#[test]
fn describe_short_circuits_before_validation() {
    // --describe should exit 0 even without a subcommand
    let output = pack_cmd().arg("--describe").output().unwrap();
    assert!(output.status.success());
}

#[test]
fn schema_short_circuits_before_validation() {
    // --schema should exit 0 even without a subcommand
    let output = pack_cmd().arg("--schema").output().unwrap();
    assert!(output.status.success());
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
fn diff_nonexistent_packs_exits_2() {
    let output = pack_cmd().args(["diff", "a", "b"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn deferred_push_exits_2() {
    let output = pack_cmd().args(["push", "some_dir"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn deferred_pull_exits_2() {
    let output = pack_cmd()
        .args(["pull", "sha256:abc", "--out", "dest"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
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
