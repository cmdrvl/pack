//! `pack verify --expect-id <PACK_ID>`: the pinned id is compared with the id verification
//! recomputes from the manifest, never with the string the manifest declares.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use pack::seal::manifest::Manifest;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const MEMBER: &str = "data.lock.json";
const USAGE_HINT: &str = "expected sha256: followed by 64 lowercase hex characters";

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

fn run(args: &[&str]) -> Output {
    pack_cmd().args(args).arg("--no-witness").output().unwrap()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Seal one small lockfile into `<tmp>/<name>` and return the directory and its pack id.
fn sealed(tmp: &TempDir, name: &str, rows: u32) -> (PathBuf, String) {
    let source = tmp.path().join(format!("{name}-src"));
    fs::create_dir(&source).unwrap();
    let artifact = source.join(MEMBER);
    fs::write(
        &artifact,
        format!(r#"{{"version":"lock.v0","rows":{rows}}}"#),
    )
    .unwrap();
    let pack_dir = tmp.path().join(name);
    let output = run(&[
        "seal",
        artifact.to_str().unwrap(),
        "--out",
        pack_dir.to_str().unwrap(),
        "--created",
        "2026-01-15T10:30:00Z",
    ]);
    assert_eq!(code(&output), 0, "{}", text(&output.stdout));
    let pack_id = read_manifest(&pack_dir).pack_id;
    (pack_dir, pack_id)
}

fn read_manifest(pack_dir: &Path) -> Manifest {
    serde_json::from_slice(&fs::read(pack_dir.join("manifest.json")).unwrap()).unwrap()
}

fn write_manifest(pack_dir: &Path, manifest: &Manifest) {
    fs::write(
        pack_dir.join("manifest.json"),
        manifest.to_canonical_bytes(),
    )
    .unwrap();
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Verify with a pinned id, in both output modes, and require the two to agree on exit code,
/// outcome, declared pack id and finding codes. Returns the JSON report and exit code.
fn verify_pinned(pack_dir: &Path, expect_id: &str) -> (Value, i32) {
    let args = ["verify", path_str(pack_dir), "--expect-id", expect_id];
    let human = run(&args);
    let json_output = run(&[args.as_slice(), &["--json"]].concat());
    let report: Value =
        serde_json::from_slice(&json_output.stdout).expect("verify --json prints one report");
    assert_eq!(code(&human), code(&json_output));

    let human = text(&human.stdout);
    let outcome = report["outcome"].as_str().unwrap();
    assert!(
        human.starts_with(&format!("pack verify: {outcome}\n")),
        "{human}"
    );
    let human_id = human
        .lines()
        .find_map(|line| line.strip_prefix("  pack_id: "));
    assert_eq!(human_id, report["pack_id"].as_str(), "{human}");
    let human_codes: Vec<&str> = human
        .lines()
        .filter_map(|line| line.strip_prefix("    - "))
        .map(|line| line.split(' ').next().unwrap())
        .collect();
    assert_eq!(human_codes, finding_codes(&report), "{human}");
    (report, code(&json_output))
}

fn finding_codes(report: &Value) -> Vec<&str> {
    report["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["code"].as_str().unwrap())
        .collect()
}

#[test]
fn a_match_is_ok_and_adds_only_positive_evidence() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pack_id) = sealed(&tmp, "p", 1);

    let (report, exit) = verify_pinned(&pack_dir, &pack_id);
    assert_eq!(exit, 0);
    assert_eq!(report["outcome"], "OK");
    assert_eq!(report["checks"]["expected_id"], true);

    // Apart from checks.expected_id the report is the unpinned one, and human text is identical.
    let plain: Value =
        serde_json::from_slice(&run(&["verify", path_str(&pack_dir), "--json"]).stdout).unwrap();
    assert!(plain["checks"].get("expected_id").is_none());
    let mut stripped = report.clone();
    stripped["checks"]
        .as_object_mut()
        .unwrap()
        .remove("expected_id");
    assert_eq!(stripped, plain);
    assert_eq!(
        run(&["verify", path_str(&pack_dir), "--expect-id", &pack_id]).stdout,
        run(&["verify", path_str(&pack_dir)]).stdout
    );
}

#[test]
fn an_intact_pack_with_another_id_is_invalid_with_only_the_expectation_finding() {
    let tmp = TempDir::new().unwrap();
    let (_, pinned) = sealed(&tmp, "pinned", 1);
    let (other_dir, other_id) = sealed(&tmp, "other", 2);

    let (report, exit) = verify_pinned(&other_dir, &pinned);
    assert_eq!(exit, 1);
    assert_eq!(report["outcome"], "INVALID");
    assert_eq!(report["pack_id"], other_id.as_str());
    assert_eq!(report["checks"]["pack_id"], true);
    assert_eq!(report["checks"]["member_hashes"], true);
    assert_eq!(report["checks"]["expected_id"], false);
    assert_eq!(
        report["invalid"],
        json!([{"code": "EXPECTED_ID_MISMATCH", "expected": pinned, "actual": other_id}])
    );
}

/// The plant a declared-string comparison would accept: the manifest declares the pinned id, and
/// every member matches its manifest entry, but the content is not the pinned bundle.
#[test]
fn a_manifest_declaring_the_pinned_id_over_other_content_is_refused() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "p", 1);
    let forged = br#"{"version":"lock.v0","rows":999}"#;
    fs::write(pack_dir.join(MEMBER), forged).unwrap();
    let mut manifest = read_manifest(&pack_dir);
    manifest.members[0].bytes_hash = format!("sha256:{}", hex::encode(Sha256::digest(forged)));
    write_manifest(&pack_dir, &manifest);
    let recomputed = manifest.recompute_pack_id();
    assert_eq!(
        manifest.pack_id, pinned,
        "the plant must keep declaring the pin"
    );
    assert_ne!(recomputed, pinned);

    let (report, exit) = verify_pinned(&pack_dir, &pinned);
    assert_eq!(exit, 1);
    assert_eq!(report["checks"]["member_hashes"], true);
    assert_eq!(report["checks"]["pack_id"], false);
    assert_eq!(report["checks"]["expected_id"], false);
    assert_eq!(
        report["invalid"],
        json!([
            {"code": "PACK_ID_MISMATCH", "expected": pinned, "actual": recomputed},
            {"code": "EXPECTED_ID_MISMATCH", "expected": pinned, "actual": recomputed}
        ])
    );
}

/// The recomputed id covers the manifest. Altered member bytes are caught by the member check,
/// so a matching expectation must not turn that into anything but INVALID.
#[test]
fn altered_member_bytes_stay_an_integrity_failure_when_the_manifest_matches() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "p", 1);
    // Still a valid lockfile, so only the byte check can notice.
    fs::write(pack_dir.join(MEMBER), r#"{"version":"lock.v0","rows":999}"#).unwrap();

    let (report, exit) = verify_pinned(&pack_dir, &pinned);
    assert_eq!(exit, 1);
    assert_eq!(report["outcome"], "INVALID");
    assert_eq!(finding_codes(&report), ["HASH_MISMATCH"]);
    assert_eq!(report["checks"]["expected_id"], true);
}

#[test]
fn only_the_declared_string_changed_is_an_integrity_failure_not_an_expectation_failure() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "p", 1);
    let mut manifest = read_manifest(&pack_dir);
    manifest.pack_id = format!("sha256:{}", "0".repeat(64));
    write_manifest(&pack_dir, &manifest);

    // Pinning the true id: the content matches the pin, the declaration does not match content.
    let (report, exit) = verify_pinned(&pack_dir, &pinned);
    assert_eq!(exit, 1);
    assert_eq!(finding_codes(&report), ["PACK_ID_MISMATCH"]);
    assert_eq!(report["checks"]["expected_id"], true);

    // Pinning the false declaration does not make it true.
    let (report, exit) = verify_pinned(&pack_dir, &manifest.pack_id);
    assert_eq!(exit, 1);
    assert_eq!(
        finding_codes(&report),
        ["PACK_ID_MISMATCH", "EXPECTED_ID_MISMATCH"]
    );
    assert_eq!(report["checks"]["expected_id"], false);
}

#[test]
fn an_integrity_failure_and_a_wrong_id_are_both_reported() {
    let tmp = TempDir::new().unwrap();
    let (_, pinned) = sealed(&tmp, "pinned", 1);
    let (other_dir, _) = sealed(&tmp, "other", 2);
    fs::write(other_dir.join("extra.txt"), "sneaky").unwrap();

    let (report, exit) = verify_pinned(&other_dir, &pinned);
    assert_eq!(exit, 1);
    assert_eq!(
        finding_codes(&report),
        ["EXTRA_MEMBER", "EXPECTED_ID_MISMATCH"]
    );
}

#[test]
fn a_manifest_refusal_is_unchanged_by_the_flag() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "p", 1);
    let mut manifest = read_manifest(&pack_dir);
    manifest.version = "pack.v9".to_string();
    let unsupported = manifest.to_canonical_bytes();
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();

    for (directory, bytes) in [
        (&empty, None),
        (&pack_dir, Some(b"NOT JSON".to_vec())),
        (&pack_dir, Some(unsupported)),
    ] {
        if let Some(bytes) = bytes {
            fs::write(directory.join("manifest.json"), bytes).unwrap();
        }
        for mode in [&[][..], &["--json"][..]] {
            let plain = run(&[&["verify", path_str(directory)][..], mode].concat());
            let flagged = run(&[
                &["verify", path_str(directory), "--expect-id", &pinned][..],
                mode,
            ]
            .concat());
            assert_eq!((code(&plain), code(&flagged)), (2, 2));
            assert_eq!(plain.stdout, flagged.stdout);
            assert!(text(&flagged.stdout).contains("E_BAD_PACK"));
        }
    }
}

#[cfg(unix)]
#[test]
fn an_unreadable_member_stays_an_io_refusal_and_still_reports_the_wrong_id() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let (_, pinned) = sealed(&tmp, "pinned", 1);
    let (other_dir, _) = sealed(&tmp, "other", 2);
    let member = other_dir.join(MEMBER);
    fs::set_permissions(&member, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&member).is_ok() {
        return; // A privileged user can read it anyway; the case cannot be planted.
    }

    let (report, exit) = verify_pinned_refusal(&other_dir, &pinned);
    fs::set_permissions(&member, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(exit, 2);
    assert_eq!(report["outcome"], "REFUSAL");
    assert_eq!(report["refusal"]["code"], "E_IO");
    assert_eq!(
        finding_codes(&report),
        ["MEMBER_IO_ERROR", "EXPECTED_ID_MISMATCH"]
    );
    assert_eq!(report["checks"]["expected_id"], false);
}

#[cfg(unix)]
fn verify_pinned_refusal(pack_dir: &Path, expect_id: &str) -> (Value, i32) {
    let args = ["verify", path_str(pack_dir), "--expect-id", expect_id];
    let human = run(&args);
    let json_output = run(&[args.as_slice(), &["--json"]].concat());
    assert_eq!(code(&human), code(&json_output));
    assert!(text(&human.stdout).contains("EXPECTED_ID_MISMATCH"));
    (
        serde_json::from_slice(&json_output.stdout).unwrap(),
        code(&json_output),
    )
}

#[test]
fn a_malformed_expected_id_is_a_usage_refusal_and_nothing_is_verified() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "p", 1);
    let hex_part = pinned.strip_prefix("sha256:").unwrap();
    let cases = [
        String::new(),
        " ".to_string(),
        hex_part.to_string(),
        "sha256:".to_string(),
        format!("blake3:{hex_part}"),
        format!("SHA256:{hex_part}"),
        format!("sha256:{}", hex_part.to_uppercase()),
        format!("sha256:{}", "A".repeat(64)),
        format!("sha256:{}", &hex_part[1..]),
        format!("sha256:{hex_part}0"),
        format!("sha256:{}g", &hex_part[1..]),
        format!("sha256:sha256:{hex_part}"),
        format!(" {pinned}"),
        format!("{pinned} "),
        format!("{pinned}\n"),
        format!("\t{pinned}"),
        format!("sha256: {hex_part}"),
    ];
    let ledger = tmp.path().join("witness.jsonl");
    for case in &cases {
        for mode in [&[][..], &["--json"][..]] {
            let output = pack_cmd()
                .env("EPISTEMIC_WITNESS", &ledger)
                .args(["verify", path_str(&pack_dir)])
                .arg(format!("--expect-id={case}"))
                .args(mode)
                .output()
                .unwrap();
            assert_eq!(code(&output), 2, "{case:?}");
            assert!(output.stdout.is_empty(), "{case:?}");
            assert!(text(&output.stderr).contains(USAGE_HINT), "{case:?}");
        }
        // The separate-argument spelling refuses too.
        let output = run(&["verify", path_str(&pack_dir), "--expect-id", case]);
        assert_eq!(code(&output), 2, "{case:?}");
        assert!(output.stdout.is_empty(), "{case:?}");
    }
    assert!(!ledger.exists(), "a usage refusal records no witness");
}

#[test]
fn a_flag_without_a_usable_value_never_falls_back_to_an_unpinned_verify() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "p", 1);
    let wrong = format!("sha256:{}", "0".repeat(64));
    let dir = path_str(&pack_dir);
    for (args, needle) in [
        (vec!["verify", dir, "--expect-id"], "a value is required"),
        (
            vec!["verify", dir, "--expect-id", "--json"],
            "a value is required",
        ),
        (
            vec!["verify", dir, "--expect-id", &wrong, "--expect-id", &pinned],
            "cannot be used multiple times",
        ),
    ] {
        let output = run(&args);
        assert_eq!(code(&output), 2, "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(
            text(&output.stderr).contains(needle),
            "{args:?}: {}",
            text(&output.stderr)
        );
    }
}

#[test]
fn witness_records_the_pin_with_the_same_outcome_in_both_modes() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "pinned", 1);
    let (other_dir, other_id) = sealed(&tmp, "other", 2);

    for (directory, declared, outcome, exit) in [
        (&pack_dir, &pinned, "OK", 0),
        (&other_dir, &other_id, "INVALID", 1),
    ] {
        for json_mode in [false, true] {
            let ledger = tmp.path().join(format!("w-{outcome}-{json_mode}.jsonl"));
            let mut command = pack_cmd();
            command.env("EPISTEMIC_WITNESS", &ledger).args([
                "verify",
                path_str(directory),
                "--expect-id",
                &pinned,
            ]);
            if json_mode {
                command.arg("--json");
            }
            assert_eq!(code(&command.output().unwrap()), exit);
            let record: Value =
                serde_json::from_str(fs::read_to_string(&ledger).unwrap().trim()).unwrap();
            assert_eq!(record["outcome"], outcome);
            assert_eq!(record["exit_code"], exit);
            assert_eq!(record["pack_id"], declared.as_str());
            assert_eq!(
                record["params"],
                json!({"pack_dir": path_str(directory), "json": json_mode, "expect_id": pinned})
            );
        }
    }

    // Without the flag the record carries no expectation.
    let ledger = tmp.path().join("w-plain.jsonl");
    let output = pack_cmd()
        .env("EPISTEMIC_WITNESS", &ledger)
        .args(["verify", path_str(&pack_dir)])
        .output()
        .unwrap();
    assert_eq!(code(&output), 0);
    let record: Value = serde_json::from_str(fs::read_to_string(&ledger).unwrap().trim()).unwrap();
    assert_eq!(
        record["params"],
        json!({"pack_dir": path_str(&pack_dir), "json": false})
    );
}

/// verify reads a pack directory. An archive is imported first; the archive file itself is not a
/// verify input, pinned or not.
#[test]
fn the_pin_holds_across_archive_import_and_an_archive_file_is_still_refused() {
    let tmp = TempDir::new().unwrap();
    let (pack_dir, pinned) = sealed(&tmp, "p", 1);
    let archive = tmp.path().join("p.tar");
    let imported = tmp.path().join("imported");
    let export = run(&[
        "archive",
        "export",
        path_str(&pack_dir),
        "--out",
        path_str(&archive),
    ]);
    assert_eq!(code(&export), 0, "{}", text(&export.stdout));
    let import = run(&[
        "archive",
        "import",
        path_str(&archive),
        "--out",
        path_str(&imported),
    ]);
    assert_eq!(code(&import), 0, "{}", text(&import.stdout));

    assert_eq!(verify_pinned(&imported, &pinned).1, 0);
    let trailing_slash = PathBuf::from(format!("{}/", path_str(&imported)));
    assert_eq!(verify_pinned(&trailing_slash, &pinned).1, 0);
    #[cfg(unix)]
    {
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&imported, &link).unwrap();
        assert_eq!(verify_pinned(&link, &pinned).1, 0);
    }

    let plain = run(&["verify", path_str(&archive), "--json"]);
    let flagged = run(&[
        "verify",
        path_str(&archive),
        "--expect-id",
        &pinned,
        "--json",
    ]);
    assert_eq!((code(&plain), code(&flagged)), (2, 2));
    assert_eq!(plain.stdout, flagged.stdout);
    assert!(text(&flagged.stdout).contains("E_BAD_PACK"));
}

#[test]
fn help_and_describe_list_the_flag() {
    let help = pack_cmd().args(["verify", "--help"]).output().unwrap();
    assert!(text(&help.stdout).contains("--expect-id <PACK_ID>"));

    let describe = pack_cmd().arg("--describe").output().unwrap();
    let operator: Value = serde_json::from_slice(&describe.stdout).unwrap();
    let verify = operator["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "verify")
        .unwrap();
    assert!(verify["options"]
        .as_array()
        .unwrap()
        .iter()
        .any(|option| option["flag"] == "--expect-id"));
}
