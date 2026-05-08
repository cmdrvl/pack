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
        .env("EPISTEMIC_WITNESS", tmp.path().join("witness.jsonl"))
        .env_remove("PACK_DATA_FABRIC_BASE_URL")
        .env_remove("PACK_DATA_FABRIC_TIMEOUT_SECS")
        .env_remove("PACK_DATA_FABRIC_RETRIES")
        .env_remove("PACK_DATA_FABRIC_RETRY_BACKOFF_MS");
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
    assert_eq!(payload["fixers"].as_array().unwrap().len(), 0);
    assert_eq!(payload["network"]["used"], false);
    assert_eq!(payload["network"]["data_fabric_env_read"], false);

    let side_effects = payload["side_effects"].as_object().unwrap();
    assert!(!side_effects.is_empty());
    for value in side_effects.values() {
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
fn doctor_fix_is_not_available() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["doctor", "--fix"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument '--fix'"));
}
