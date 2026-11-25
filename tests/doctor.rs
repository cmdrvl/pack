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
    assert_eq!(payload["findings"].as_array().map(Vec::len), Some(6));
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

    let side_effects = capabilities["side_effects"].as_object().unwrap();
    assert_eq!(side_effects["reads_files"], true);
    assert_eq!(side_effects["writes_files"], true);
    assert_eq!(side_effects["network"], false);
    assert_eq!(side_effects["writes_witness"], true);

    let composition = &capabilities["capabilities"]["composition"];
    assert_eq!(composition["family"]["name"], "cmdrvl-spine");
    assert_eq!(composition["position"], 5);
    assert_eq!(composition["produces"][0], "pack.v0 directory");
    assert!(composition["accepts"]
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value == "lock.v0 JSON")));
    assert_eq!(capabilities["capabilities"]["doctor"]["read_only"], true);
    assert_eq!(
        capabilities["capabilities"]["doctor"]["fix_mode"],
        "not_available"
    );
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
fn top_level_robot_triage_is_a_doctor_v1_witness() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .arg("--robot-triage")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(!tmp.path().join("witness.jsonl").exists());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(payload["mode"], "triage");
    assert_eq!(payload["tool"], "pack");
    assert_eq!(payload["healthy"], true);
}

#[test]
fn robot_docs_guide_preserves_agent_operating_content() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["robot-docs", "guide"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "pack --robot-triage",
        "pack capabilities --json",
        "pack robot-docs guide",
        "pack doctor triage --json",
        "pack seal ... --out <DIR>",
        "--output",
        "NO_COLOR",
        "Repair policy",
    ] {
        assert!(stdout.contains(expected), "robot-docs omitted {expected:?}");
    }
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
        command
            .arg("--help")
            .env("CLICOLOR_FORCE", "1")
            .env("TERM", "xterm-256color");
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
fn unhealthy_doctor_state_exits_one() {
    let tmp = tempfile::tempdir().unwrap();
    let output = isolated_pack_cmd(&tmp)
        .args(["doctor", "--json"])
        .env("SOURCE_DATE_EPOCH", "not-unix-seconds")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(payload["mode"], "health");
    assert_eq!(payload["healthy"], false);
    assert!(payload["findings"].as_array().is_some_and(|findings| {
        findings.iter().any(|finding| {
            finding["id"] == "pack.source_date_epoch" && finding["severity"] == "error"
        })
    }));
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
