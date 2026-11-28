use std::fs;

use pack::archive::execute_archive_export;
use pack::render::{render_in_memory, RenderFormat};
use pack::seal::command::execute_seal;
use pack::seal::manifest::Manifest;
use pack::seal::seal_to_tar_bytes;
use pack::verify::{execute_verify, verify_tar_bytes, VerifyOutcome};

fn members() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("nov.lock.json", br#"{"version":"lock.v0","rows":5}"#),
        (
            "rvl.report.json",
            br#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE","counts":{"rows_old":1,"rows_new":1,"rows_aligned":1},"contributors":[]}"#,
        ),
    ]
}

#[test]
fn seal_bytes_match_native_archive_and_verify_report() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let archive_output = tempfile::tempdir().unwrap();
    let lock = source.path().join("nov.lock.json");
    let report = source.path().join("rvl.report.json");
    fs::write(&lock, members()[0].1).unwrap();
    fs::write(&report, members()[1].1).unwrap();
    let pack_dir = output.path().join("pack");

    let sealed = execute_seal(
        &[lock.clone(), report.clone()],
        Some(&pack_dir),
        None,
        Some("2026-01-15T10:30:00Z"),
        None,
    )
    .unwrap();
    let native_archive = archive_output.path().join("pack.tar");
    execute_archive_export(&pack_dir, &native_archive).unwrap();
    let native_tar = fs::read(&native_archive).unwrap();

    let memory_tar = seal_to_tar_bytes(&members(), "2026-01-15T10:30:00Z", None).unwrap();
    assert_eq!(memory_tar, native_tar);

    let (native_report, native_code) = execute_verify(&pack_dir, true);
    let memory_report = verify_tar_bytes(&memory_tar);
    assert_eq!(native_code, 0);
    assert_eq!(memory_report.outcome, VerifyOutcome::OK);
    assert_eq!(memory_report.to_json(), native_report);
    assert_eq!(
        memory_report.pack_id.as_deref(),
        Some(sealed.pack_id.as_str())
    );
}

#[test]
fn render_bytes_match_native_render_for_md_and_html() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let lock = source.path().join("nov.lock.json");
    let report = source.path().join("rvl.report.json");
    fs::write(&lock, members()[0].1).unwrap();
    fs::write(&report, members()[1].1).unwrap();
    let pack_dir = output.path().join("pack");
    execute_seal(
        &[lock.clone(), report.clone()],
        Some(&pack_dir),
        Some("memory parity".to_string()),
        Some("2026-01-15T10:30:00Z"),
        None,
    )
    .unwrap();

    let manifest: Manifest =
        serde_json::from_slice(&fs::read(pack_dir.join("manifest.json")).unwrap()).unwrap();
    let lock_bytes = fs::read(pack_dir.join("nov.lock.json")).unwrap();
    let report_bytes = fs::read(pack_dir.join("rvl.report.json")).unwrap();
    let member_bytes = vec![
        ("nov.lock.json", lock_bytes.as_slice()),
        ("rvl.report.json", report_bytes.as_slice()),
    ];

    for (format, suffix) in [(RenderFormat::Md, "md"), (RenderFormat::Html, "html")] {
        let native_path = output.path().join(format!("native.{suffix}"));
        pack::render::execute_render(
            &pack_dir,
            match format {
                RenderFormat::Md => pack::cli::RenderFormat::Md,
                RenderFormat::Html => pack::cli::RenderFormat::Html,
            },
            &native_path,
        )
        .unwrap();
        let native = fs::read(&native_path).unwrap();
        let memory = render_in_memory(&manifest, &member_bytes, format).unwrap();
        assert_eq!(memory, native, "{suffix} output differs");
    }
}

#[test]
fn malformed_tar_is_a_typed_refusal_without_filesystem_access() {
    let tar = seal_to_tar_bytes(&members(), "2026-01-15T10:30:00Z", None).unwrap();
    let report = verify_tar_bytes(&tar[..tar.len() - 1]);
    assert_eq!(report.outcome, VerifyOutcome::REFUSAL);
    assert_eq!(report.refusal.as_ref().unwrap()["code"], "E_BAD_PACK");
    assert!(report.invalid.is_empty());
}

#[test]
fn seal_rejects_empty_duplicate_and_unsafe_members() {
    assert_eq!(
        seal_to_tar_bytes(&[], "2026-01-15T10:30:00Z", None)
            .unwrap_err()
            .refusal
            .code,
        "E_EMPTY"
    );

    let duplicate = [("same.txt", b"a".as_slice()), ("same.txt", b"b".as_slice())];
    assert_eq!(
        seal_to_tar_bytes(&duplicate, "2026-01-15T10:30:00Z", None)
            .unwrap_err()
            .refusal
            .code,
        "E_DUPLICATE"
    );

    let unsafe_member = [("../outside.txt", b"a".as_slice())];
    assert_eq!(
        seal_to_tar_bytes(&unsafe_member, "2026-01-15T10:30:00Z", None)
            .unwrap_err()
            .refusal
            .code,
        "E_BAD_PACK"
    );
}
