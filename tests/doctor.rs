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

fn doctor_report(tmp: &tempfile::TempDir, args: &[&str], mode: &str) -> Value {
    let output = isolated_pack_cmd(tmp)
        .args(args)
        .output()
        .expect("doctor command should run");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("doctor stdout is JSON");
    assert_eq!(payload["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(payload["mode"], mode);
    assert_eq!(payload["tool"], "pack");
    assert_eq!(payload["tool_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(payload["healthy"], true);
    assert!(payload["findings"].is_array());
    assert!(!tmp.path().join("witness.jsonl").exists());
    payload
}

#[test]
fn doctor_health_uses_family_schema_and_is_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = doctor_report(&tmp, &["doctor", "--json"], "health");
    assert_eq!(payload["findings"].as_array().map(Vec::len), Some(5));
}

#[test]
fn legacy_doctor_health_alias_remains_supported() {
    let tmp = tempfile::tempdir().unwrap();
    doctor_report(&tmp, &["doctor", "health", "--json"], "health");
}

#[test]
fn doctor_capabilities_embeds_the_exact_operator_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let doctor = doctor_report(&tmp, &["doctor", "capabilities", "--json"], "capabilities");
    let capabilities_output = isolated_pack_cmd(&tmp)
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    let capabilities: Value = serde_json::from_slice(&capabilities_output.stdout).unwrap();
    assert_eq!(doctor["contract"], capabilities);
    assert_eq!(capabilities["schema_version"], "operator.v1");
}

#[test]
fn doctor_triage_has_next_actions_and_legacy_alias_works() {
    let tmp = tempfile::tempdir().unwrap();
    let triage = doctor_report(&tmp, &["doctor", "triage", "--json"], "triage");
    assert!(triage["next_actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|action| action == "pack capabilities --json")
    }));

    let legacy = doctor_report(&tmp, &["doctor", "--robot-triage"], "triage");
    assert_eq!(legacy["next_actions"], triage["next_actions"]);
}

#[test]
fn top_level_capabilities_matches_describe() {
    let tmp = tempfile::tempdir().unwrap();
    let describe = isolated_pack_cmd(&tmp).arg("--describe").output().unwrap();
    let capabilities = isolated_pack_cmd(&tmp)
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    let describe_json: Value = serde_json::from_slice(&describe.stdout).unwrap();
    let capabilities_json: Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    assert_eq!(describe_json, capabilities_json);
}

#[test]
fn seal_accepts_canonical_out_alias_and_keeps_output_compatibility() {
    let tmp = tempfile::tempdir().unwrap();
    let artifact = tmp.path().join("artifact.json");
    std::fs::write(&artifact, r#"{"version":"lock.v0","rows":1}"#).unwrap();
    let pack_dir = tmp.path().join("sealed");
    let output = isolated_pack_cmd(&tmp)
        .args([
            "--no-witness",
            "seal",
            artifact.to_str().unwrap(),
            "--out",
            pack_dir.to_str().unwrap(),
            "--created",
            "2026-01-15T10:30:00Z",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(pack_dir.join("manifest.json").exists());

    let compatibility_dir = tmp.path().join("compat");
    let compatibility = isolated_pack_cmd(&tmp)
        .args([
            "--no-witness",
            "seal",
            artifact.to_str().unwrap(),
            "--output",
            compatibility_dir.to_str().unwrap(),
            "--created",
            "2026-01-15T10:30:00Z",
        ])
        .output()
        .unwrap();
    assert_eq!(compatibility.status.code(), Some(0));
}

#[test]
fn no_color_and_non_tty_help_are_ansi_free() {
    for no_color in [true, false] {
        let mut command = pack_cmd();
        command.arg("--help");
        if no_color {
            command.env("NO_COLOR", "1").env("TERM", "dumb");
        } else {
            command.env_remove("NO_COLOR").env_remove("TERM");
        }
        let output = command.output().unwrap();
        assert!(!output.stdout.contains(&0x1b), "help stdout contained ANSI");
        assert!(!output.stderr.contains(&0x1b), "help stderr contained ANSI");
    }
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
    assert!(!tmp.path().join("witness.jsonl").exists());
}
