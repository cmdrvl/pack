use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

fn pack_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

fn full_pack() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let inputs = [
        (
            "lock.json",
            r#"{"version":"lock.v0","lock_hash":"sha256:lock","inputs":["source.csv"]}"#,
        ),
        ("shape.json", r#"{"version":"shape.v0","columns":2}"#),
        (
            "rvl.json",
            r#"{"version":"rvl.v0","outcome":"REAL_CHANGE","counts":{"rows_old":4,"rows_new":5,"rows_aligned":4},"contributors":[{"contribution":2.5,"share":0.5,"column":"RAW_SECRET"}],"field_changes":[{"old":"RAW_SECRET","new":"OTHER_SECRET"}]}"#,
        ),
        (
            "verify.json",
            r#"{"version":"verify.v0","outcome":"PASS","passed":4,"failed":0}"#,
        ),
        (
            "assess.json",
            r#"{"tool":"assess","version":"assess.v0","decision_band":"PROCEED","policy":{"id":"policy.one","version":1,"sha256":"sha256:policy"},"matched_rule":"rule.allow","required_tools":[],"observed_tools":[],"risk_factors":[{"code":"rule.allow"}],"epistemic_basis":[],"refusal":null}"#,
        ),
        (
            "airlock.json",
            r#"{"manifest_version":"airlock.v0","achieved_claim":"RAW_DOCUMENT_ABSENT","boundary_mode":"ANNOTATED","LOCAL_READS_CLEAN":true,"bytes_crossed":["none"],"not_crossed":["raw"],"signer":"operator"}"#,
        ),
        (
            "benchmark.json",
            r#"{"version":"benchmark.v0","outcome":"PASS","passed":3,"failed":0}"#,
        ),
    ];

    let mut paths = Vec::new();
    for (name, content) in inputs {
        let path = tmp.path().join(name);
        std::fs::write(&path, content).unwrap();
        paths.push(path);
    }

    let pack_dir = tmp.path().join("pack");
    let mut command = pack_cmd();
    command.args(["--no-witness", "seal"]);
    command.args(paths.iter().map(|path| path.to_str().unwrap()));
    command.args([
        "--out",
        pack_dir.to_str().unwrap(),
        "--created",
        "2026-01-15T10:30:00Z",
    ]);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "seal stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (tmp, pack_dir)
}

#[test]
fn golden_html_and_markdown_are_self_contained_and_redacted() {
    let (_tmp, pack_dir) = full_pack();
    let html_one = pack_dir.parent().unwrap().join("audit-one.html");
    let html_two = pack_dir.parent().unwrap().join("audit-two.html");
    let markdown = pack_dir.parent().unwrap().join("audit.md");

    for output_path in [&html_one, &html_two] {
        let output = pack_cmd()
            .args([
                "--no-witness",
                "render",
                pack_dir.to_str().unwrap(),
                "--format",
                "html",
                "--out",
                output_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "render stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let html = std::fs::read_to_string(&html_one).unwrap();
    assert_eq!(html, std::fs::read_to_string(&html_two).unwrap());
    assert!(html.contains("Verification badge: VERIFIED"));
    assert!(html.contains("Pack seal hash"));
    assert!(html.contains("PROCEED"));
    assert!(html.contains("Top numeric contributors"));
    assert!(html.contains("Airlock boundary"));
    assert!(!html.contains("RAW_SECRET"));
    assert!(!html.contains("https://"));
    assert!(!html.contains("http://"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("src="));
    assert!(!html.contains("@import"));

    let output = pack_cmd()
        .args([
            "--no-witness",
            "render",
            pack_dir.to_str().unwrap(),
            "--format",
            "md",
            "--out",
            markdown.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let markdown_text = std::fs::read_to_string(markdown).unwrap();
    assert!(markdown_text.contains("# Pack audit dossier"));
    assert!(markdown_text.contains("## Member inventory"));
    assert!(markdown_text.contains("## Assess decisions"));
    assert!(markdown_text.contains("## RVL comparison"));
    assert!(markdown_text.contains("## Verify and benchmark witnesses"));
    assert!(markdown_text.contains("## Lock input provenance"));
    assert!(markdown_text.contains("## Airlock boundary"));
    assert!(!markdown_text.contains("RAW_SECRET"));
}

#[test]
fn render_refuses_tampered_member_before_writing() {
    let (tmp, pack_dir) = full_pack();
    std::fs::write(pack_dir.join("rvl.json"), "TAMPERED_RAW_SECRET").unwrap();
    let output_path = tmp.path().join("should-not-exist.html");

    let output = pack_cmd()
        .args([
            "--no-witness",
            "render",
            pack_dir.to_str().unwrap(),
            "--format",
            "html",
            "--out",
            output_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_BAD_PACK");
    assert!(!output_path.exists());
}
