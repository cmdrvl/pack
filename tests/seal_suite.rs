use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

// ---------------------------------------------------------------------------
// Golden snapshot: deterministic member ordering + manifest shape
// ---------------------------------------------------------------------------

/// Seal the committed fixtures and assert the manifest matches the golden
/// snapshot stored in fixtures/packs/valid/manifest.json.
///
/// This catches regressions in member ordering, type detection, canonical
/// serialization, and the self-hash procedure.
#[test]
fn seal_fixtures_produces_golden_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path().join("golden");

    let output = pack_cmd()
        .args([
            "seal",
            "fixtures/artifacts/nov.lock.json",
            "fixtures/artifacts/dec.lock.json",
            "fixtures/artifacts/shape.report.json",
            "fixtures/artifacts/rvl.report.json",
            "fixtures/artifacts/verify.report.json",
            "fixtures/artifacts/rules.json",
            "fixtures/artifacts/profile.yaml",
            "fixtures/artifacts/unknown.txt",
            "fixtures/artifacts/nested_registry",
            "--output",
            output_dir.to_str().unwrap(),
            "--note",
            "fixture: valid evidence pack",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "seal failed: {:?}", output);

    // Read both manifests
    let golden = std::fs::read_to_string("fixtures/packs/valid/manifest.json").unwrap();
    let produced = std::fs::read_to_string(output_dir.join("manifest.json")).unwrap();

    // Parse to compare structure (created timestamp will differ)
    let golden_val: serde_json::Value = serde_json::from_str(&golden).unwrap();
    let produced_val: serde_json::Value = serde_json::from_str(&produced).unwrap();

    // Member ordering and content must match exactly. The golden fixture may
    // have been sealed by an older pack release, so tool_version should track
    // the current binary rather than the historical fixture.
    assert_eq!(golden_val["version"], produced_val["version"]);
    assert_eq!(golden_val["member_count"], produced_val["member_count"]);
    assert_eq!(golden_val["note"], produced_val["note"]);
    assert_eq!(produced_val["tool_version"], env!("CARGO_PKG_VERSION"));

    // Members array must be identical (same order, same hashes, same types)
    assert_eq!(golden_val["members"], produced_val["members"]);
}

/// Member paths in the manifest are sorted bytewise.
#[test]
fn manifest_members_are_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path().join("sorted");

    // Create files with names that exercise sorting
    let art_dir = tmp.path().join("arts");
    std::fs::create_dir(&art_dir).unwrap();
    std::fs::write(art_dir.join("z.json"), r#"{"version":"lock.v0"}"#).unwrap();
    std::fs::write(art_dir.join("a.json"), r#"{"version":"lock.v0"}"#).unwrap();
    std::fs::write(art_dir.join("m.json"), r#"{"version":"rvl.v0"}"#).unwrap();

    let output = pack_cmd()
        .args([
            "seal",
            art_dir.join("z.json").to_str().unwrap(),
            art_dir.join("a.json").to_str().unwrap(),
            art_dir.join("m.json").to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let manifest_content = std::fs::read_to_string(output_dir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let paths: Vec<&str> = manifest["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["path"].as_str().unwrap())
        .collect();

    assert_eq!(paths, vec!["a.json", "m.json", "z.json"]);
}

// ---------------------------------------------------------------------------
// pack_id self-hash contract
// ---------------------------------------------------------------------------

/// The pack_id in the manifest is the SHA256 of the canonical manifest with
/// pack_id set to "". Verify this by recomputing from the serialized manifest.
#[test]
fn pack_id_self_hash_contract_holds() {
    let manifest_content = std::fs::read_to_string("fixtures/packs/valid/manifest.json").unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let claimed_pack_id = manifest["pack_id"].as_str().unwrap().to_string();

    // Set pack_id to "" and recompute
    manifest["pack_id"] = serde_json::Value::String(String::new());

    // Canonical JSON: sorted keys, no whitespace
    let canonical = sorted_json(&manifest);
    let hash = sha256_hex(canonical.as_bytes());
    let expected = format!("sha256:{hash}");

    assert_eq!(
        claimed_pack_id, expected,
        "pack_id does not match self-hash"
    );
}

// ---------------------------------------------------------------------------
// Collision and refusal tests (integration level via CLI)
// ---------------------------------------------------------------------------

/// Duplicate member paths refuse with E_DUPLICATE.
#[test]
fn seal_duplicate_members_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("data.json");
    std::fs::write(&file, r#"{"version":"lock.v0"}"#).unwrap();

    let output = pack_cmd()
        .args([
            "seal",
            file.to_str().unwrap(),
            file.to_str().unwrap(), // same file twice = same basename
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("E_DUPLICATE"));
}

/// Empty artifact list refuses with E_EMPTY.
#[test]
fn seal_empty_input_refuses() {
    // This would normally be caught by clap requiring at least one arg,
    // but we test via the library path in command tests.
    // CLI-level: "pack seal" with no artifacts is a clap error.
    let output = pack_cmd().args(["seal"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

/// Non-empty output directory refuses.
#[test]
fn seal_refuses_non_empty_output_dir() {
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
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("E_IO"));
}

// ---------------------------------------------------------------------------
// No-partial-output staging
// ---------------------------------------------------------------------------

/// When seal fails (e.g. nonexistent input), no output directory is created.
#[test]
fn seal_failure_leaves_no_output_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path().join("should_not_exist");

    let output = pack_cmd()
        .args([
            "seal",
            "/nonexistent/file.json",
            "--output",
            output_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !output_dir.exists(),
        "Output dir should not exist after failed seal"
    );
}

// ---------------------------------------------------------------------------
// Seal output format
// ---------------------------------------------------------------------------

/// Seal stdout format: "PACK_CREATED sha256:..." on first line, dir on second.
#[test]
fn seal_stdout_format() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let out = tmp.path().join("pack_out");

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
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("PACK_CREATED sha256:"));
    assert!(lines[1].contains("pack_out"));
}

#[test]
fn seal_without_output_uses_cmdrvl_state_pack_and_migrates_legacy_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&work).unwrap();

    let legacy_marker = work.join("pack").join("legacy").join("marker.txt");
    std::fs::create_dir_all(legacy_marker.parent().unwrap()).unwrap();
    std::fs::write(&legacy_marker, "legacy").unwrap();

    let artifact =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/artifacts/nov.lock.json");

    let output = pack_cmd()
        .current_dir(&work)
        .env("HOME", &home)
        .env_remove("EPISTEMIC_WITNESS")
        .args(["seal", artifact.to_str().unwrap(), "--no-witness"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "seal failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 2);
    let pack_id = lines[0].strip_prefix("PACK_CREATED ").unwrap();
    let expected_dir = home.join(".cmdrvl/state/pack").join(pack_id);
    assert_eq!(lines[1], expected_dir.display().to_string());
    assert!(expected_dir.join("manifest.json").exists());
    assert_eq!(
        std::fs::read_to_string(home.join(".cmdrvl/state/pack/legacy/marker.txt")).unwrap(),
        "legacy"
    );

    let migrations =
        std::fs::read_to_string(home.join(".cmdrvl/migrations/applied.jsonl")).unwrap();
    assert!(migrations.contains("\"path_class\":\"pack_default_output_dir\""));
    assert!(migrations.contains("\"action\":\"copied_legacy_to_canonical\""));
    let notices =
        std::fs::read_to_string(home.join(".cmdrvl/notices/deprecated-paths.jsonl")).unwrap();
    assert!(notices.contains("\"path_class\":\"pack_default_output_dir\""));
}

/// Seal with --note includes the note in the manifest.
#[test]
fn seal_note_in_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0"}"#).unwrap();
    let out = tmp.path().join("noted");

    let output = pack_cmd()
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--note",
            "Q4 2025 reconciliation",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let manifest_content = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    assert_eq!(manifest["note"], "Q4 2025 reconciliation");
}

/// --created makes identical inputs reproduce the same manifest bytes and pack_id.
#[test]
fn seal_created_flag_makes_manifest_reproducible() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let out_a = tmp.path().join("pack_a");
    let out_b = tmp.path().join("pack_b");

    for out in [&out_a, &out_b] {
        let output = pack_cmd()
            .env("SOURCE_DATE_EPOCH", "not-used")
            .args([
                "seal",
                art.to_str().unwrap(),
                "--output",
                out.to_str().unwrap(),
                "--created",
                "2026-01-15T10:30:00Z",
                "--no-witness",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "seal failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let manifest_a = std::fs::read_to_string(out_a.join("manifest.json")).unwrap();
    let manifest_b = std::fs::read_to_string(out_b.join("manifest.json")).unwrap();
    assert_eq!(manifest_a, manifest_b);

    let parsed: serde_json::Value = serde_json::from_str(&manifest_a).unwrap();
    assert_eq!(parsed["created"], "2026-01-15T10:30:00Z");
    assert!(parsed["pack_id"].as_str().unwrap().starts_with("sha256:"));
}

/// SOURCE_DATE_EPOCH controls created when --created is absent.
#[test]
fn seal_uses_source_date_epoch_when_created_flag_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let out = tmp.path().join("pack_out");

    let output = pack_cmd()
        .env("SOURCE_DATE_EPOCH", "42")
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let manifest_content = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    assert_eq!(manifest["created"], "1970-01-01T00:00:42Z");
}

/// Invalid --created values refuse with a structured JSON envelope.
#[test]
fn seal_invalid_created_refuses_with_json_envelope() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let out = tmp.path().join("pack_out");

    let output = pack_cmd()
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--created",
            "not-rfc3339",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!out.exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["outcome"], "REFUSAL");
    assert_eq!(envelope["refusal"]["code"], "E_IO");
    assert_eq!(envelope["refusal"]["detail"]["flag"], "--created");
}

/// --outcome stores a canonical outcome tag in the manifest.
#[test]
fn seal_outcome_in_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let out = tmp.path().join("outcome_pack");

    let output = pack_cmd()
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--created",
            "2026-01-15T10:30:00Z",
            "--outcome",
            "cmdrvl://catalog/example/canon/entity/outcome/NO_REAL_CHANGE",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let manifest_content = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    assert_eq!(
        manifest["primary_outcome_tag"],
        "cmdrvl://catalog/example/canon/entity/outcome/NO_REAL_CHANGE"
    );
}

/// --outcome changes pack_id while staying stable for repeated seals with same inputs.
#[test]
fn seal_outcome_changes_pack_id_and_is_reproducible() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();

    let base_out = tmp.path().join("base");
    let out_a = tmp.path().join("with_outcome_a");
    let out_b = tmp.path().join("with_outcome_b");

    let seal = |out: &std::path::Path, outcome: Option<&str>| -> serde_json::Value {
        let mut args = vec![
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--created",
            "2026-01-15T10:30:00Z",
            "--no-witness",
        ];
        if let Some(tag) = outcome {
            args.push("--outcome");
            args.push(tag);
        }
        let output = pack_cmd().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "seal failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let manifest_content = std::fs::read_to_string(out.join("manifest.json")).unwrap();
        serde_json::from_str(&manifest_content).unwrap()
    };

    let no_outcome = seal(&base_out, None);
    let with_outcome_a = seal(
        &out_a,
        Some("cmdrvl://catalog/example/canon/entity/outcome/NO_REAL_CHANGE"),
    );
    let with_outcome_b = seal(
        &out_b,
        Some("cmdrvl://catalog/example/canon/entity/outcome/NO_REAL_CHANGE"),
    );

    assert_ne!(no_outcome["pack_id"], with_outcome_a["pack_id"]);
    assert_eq!(with_outcome_a["pack_id"], with_outcome_b["pack_id"]);
}

/// Non-canonical --outcome values refuse with a structured envelope.
#[test]
fn seal_invalid_outcome_refuses_with_json_envelope() {
    let tmp = tempfile::tempdir().unwrap();
    let art = tmp.path().join("data.json");
    std::fs::write(&art, r#"{"version":"lock.v0","rows":5}"#).unwrap();
    let out = tmp.path().join("pack_out");

    let output = pack_cmd()
        .args([
            "seal",
            art.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--outcome",
            "outcome:bdc",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!out.exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["outcome"], "REFUSAL");
    assert_eq!(envelope["refusal"]["code"], "E_IO");
    assert_eq!(envelope["refusal"]["detail"]["flag"], "--outcome");
    assert_eq!(envelope["refusal"]["detail"]["expected"], "cmdrvl://...");
}

/// Members bytes in sealed pack match source bytes exactly.
#[test]
fn sealed_member_bytes_match_source() {
    let source_content = std::fs::read("fixtures/artifacts/rules.json").unwrap();

    let manifest_content = std::fs::read_to_string("fixtures/packs/valid/manifest.json").unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();

    // Find rules.json member hash from manifest
    let members = manifest["members"].as_array().unwrap();
    let rules_member = members.iter().find(|m| m["path"] == "rules.json").unwrap();
    let expected_hash = rules_member["bytes_hash"].as_str().unwrap();

    // Verify hash matches actual file content
    let actual_hash = format!("sha256:{}", sha256_hex(&source_content));
    assert_eq!(expected_hash, actual_hash);

    // Verify the sealed copy matches source
    let sealed_content = std::fs::read("fixtures/packs/valid/rules.json").unwrap();
    assert_eq!(source_content, sealed_content);
}

/// Nested directory members preserve relative paths.
#[test]
fn nested_directory_member_paths() {
    let manifest_content = std::fs::read_to_string("fixtures/packs/valid/manifest.json").unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let members = manifest["members"].as_array().unwrap();
    let paths: Vec<&str> = members
        .iter()
        .map(|m| m["path"].as_str().unwrap())
        .collect();

    // Nested registry files should have "nested_registry/" prefix
    assert!(paths.contains(&"nested_registry/loans.csv"));
    assert!(paths.contains(&"nested_registry/registry.json"));
}

/// Type detection results are consistent with fixture expectations.
#[test]
fn seal_type_detection_matches_expectations() {
    let manifest_content = std::fs::read_to_string("fixtures/packs/valid/manifest.json").unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let members = manifest["members"].as_array().unwrap();

    let type_map: HashMap<&str, &str> = members
        .iter()
        .map(|m| (m["path"].as_str().unwrap(), m["type"].as_str().unwrap()))
        .collect();

    assert_eq!(type_map["nov.lock.json"], "lockfile");
    assert_eq!(type_map["dec.lock.json"], "lockfile");
    assert_eq!(type_map["shape.report.json"], "report");
    assert_eq!(type_map["rvl.report.json"], "report");
    assert_eq!(type_map["verify.report.json"], "report");
    assert_eq!(type_map["rules.json"], "rules");
    assert_eq!(type_map["profile.yaml"], "profile");
    assert_eq!(type_map["unknown.txt"], "other");
    assert_eq!(type_map["nested_registry/registry.json"], "registry");
    assert_eq!(type_map["nested_registry/loans.csv"], "registry");
}

#[cfg(unix)]
#[test]
fn seal_preserves_literal_backslashes_in_directory_member_names() {
    let tmp = tempfile::tempdir().unwrap();
    let input_dir = tmp.path().join("inputs");
    let output_dir = tmp.path().join("sealed");
    std::fs::create_dir(&input_dir).unwrap();
    std::fs::write(
        input_dir.join(r"literal\member.json"),
        r#"{"version":"lock.v0"}"#,
    )
    .unwrap();

    let output = pack_cmd()
        .args([
            "seal",
            input_dir.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "seal failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest_content = std::fs::read_to_string(output_dir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let paths: Vec<&str> = manifest["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["path"].as_str().unwrap())
        .collect();

    assert_eq!(paths, vec![r"inputs/literal\member.json"]);
    assert!(output_dir
        .join("inputs")
        .join(r"literal\member.json")
        .exists());
    assert!(!output_dir
        .join("inputs")
        .join("literal/member.json")
        .exists());
}

// ---------------------------------------------------------------------------
// Helpers (local copies of canonical JSON / SHA256 for verification)
// ---------------------------------------------------------------------------

fn sorted_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let entries: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap(),
                        sorted_json(&map[*k])
                    )
                })
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        serde_json::Value::Array(arr) => {
            let entries: Vec<String> = arr.iter().map(sorted_json).collect();
            format!("[{}]", entries.join(","))
        }
        _ => serde_json::to_string(value).unwrap(),
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
