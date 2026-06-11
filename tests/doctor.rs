use serde_json::Value;
use std::process::Command;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

fn isolated_pack_cmd(tmp: &tempfile::TempDir) -> Command {
    let mut command = pack_cmd();
    command
        .env("HOME", tmp.path())
        .env("USERPROFILE", tmp.path())
        .env("EPISTEMIC_WITNESS", tmp.path().join("witness.jsonl"));
    command
}

#[test]
fn doctor_health_json_exits_zero_without_writing_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["doctor", "health", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema_version"], "pack.doctor.health.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["tool"], "pack");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["summary"]["error"], 0);
    assert!(!tmp.path().join("witness.jsonl").exists());
}

#[test]
fn doctor_capabilities_json_advertises_no_fixers_or_side_effects() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["doctor", "capabilities", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema_version"], "pack.doctor.capabilities.v1");
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["fix_mode"]["status"], "not_available");
    assert_eq!(payload["fix_mode"]["available"], false);
    assert_eq!(payload["fixers"].as_array().unwrap().len(), 0);
    assert_eq!(payload["network"]["used"], false);
    assert_eq!(payload["network"]["config_env_read"], false);
    assert_eq!(
        payload["agent_surfaces"]["capabilities"]["command"],
        "pack capabilities --json"
    );
    assert_eq!(
        payload["agent_surfaces"]["robot_docs"]["command"],
        "pack robot-docs guide"
    );
    assert_eq!(
        payload["side_effects"]["by_command"]["pack capabilities --json"]["writes_witness_ledger"],
        false
    );

    let side_effects = payload["side_effects"].as_object().unwrap();
    assert!(!side_effects.is_empty());
    for value in side_effects.values().filter(|value| value.is_boolean()) {
        assert_eq!(value, false);
    }
}

#[test]
fn doctor_robot_triage_json_is_machine_readable() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["doctor", "--robot-triage"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema_version"], "pack.doctor.triage.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["health"]["schema_version"], "pack.doctor.health.v1");
    assert_eq!(
        payload["capabilities"]["schema_version"],
        "pack.doctor.capabilities.v1"
    );
}

#[test]
fn top_level_robot_triage_json_is_machine_readable() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .arg("--robot-triage")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(!tmp.path().join("witness.jsonl").exists());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema_version"], "pack.doctor.triage.v1");
    assert_eq!(payload["ok"], true);
    assert_eq!(
        payload["capabilities"]["agent_surfaces"]["robot_triage"]["command"],
        "pack --robot-triage"
    );
}

#[test]
fn top_level_capabilities_json_advertises_agent_surfaces() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["capabilities", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema_version"], "pack.doctor.capabilities.v1");
    assert_eq!(payload["read_only"], true);
    assert_eq!(
        payload["agent_surfaces"]["capabilities"]["command"],
        "pack capabilities --json"
    );
    assert_eq!(
        payload["agent_surfaces"]["robot_docs"]["command"],
        "pack robot-docs guide"
    );
    assert_eq!(payload["composition"]["family"]["name"], "cmdrvl-spine");
    assert_eq!(payload["composition"]["position"], 5);
    assert_eq!(payload["composition"]["produces"][0], "pack.v0 directory");
    assert!(payload["composition"]["accepts"]
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value == "lock.v0 JSON")));
    assert_eq!(
        payload["side_effects"]["by_command"]["pack capabilities --json"]["uses_network"],
        false
    );
}

#[test]
fn top_level_robot_docs_guide_names_agent_surface() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["robot-docs", "guide"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pack --robot-triage"));
    assert!(stdout.contains("pack capabilities --json"));
    assert!(stdout.contains("pack robot-docs guide"));
    assert!(stdout.contains("Composition:"));
    assert!(stdout.contains("vacuum --json <ROOT>... | hashbytes | fingerprint --fp <ID>"));
    assert!(stdout.contains("pack seal dataset.lock.json shape.report.json rvl.report.json"));
    assert!(stdout.contains("pack doctor --fix` is unavailable"));
}

#[test]
fn doctor_fix_is_not_available() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["doctor", "--fix"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pack doctor --fix is unavailable"));
    assert!(stderr.contains("pack --robot-triage"));
    assert!(stderr.contains("pack capabilities --json"));
    assert!(stderr.contains("pack robot-docs guide"));
    assert!(!tmp.path().join("witness.jsonl").exists());
}
