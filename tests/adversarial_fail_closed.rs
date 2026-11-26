use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const CHILD_TIMEOUT: Duration = Duration::from_secs(5);
const TAR_BLOCK_SIZE: usize = 512;

#[derive(Clone, Copy)]
struct TarEntry<'a> {
    path: &'a str,
    typeflag: u8,
    bytes: &'a [u8],
    link_name: &'a str,
}

fn pack_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pack"))
}

fn run_bounded<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = pack_command()
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pack child");
    let started = Instant::now();

    loop {
        if child.try_wait().expect("poll pack child").is_some() {
            return child.wait_with_output().expect("collect pack child output");
        }
        let timed_out = started.elapsed() >= CHILD_TIMEOUT;
        if timed_out {
            let _ = child.kill();
            let child_id = child.id();
            let output = child
                .wait_with_output()
                .expect("collect timed-out pack child output");
            assert!(
                !timed_out,
                "pack child exceeded {CHILD_TIMEOUT:?}: {}",
                child_id
            );
            return output;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn run_bounded_with_stdin<I, S>(args: I, input: &[u8]) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = pack_command()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pack stdin child");
    let mut input_writer = child.stdin.take().expect("pack stdin should be piped");
    let input = input.to_vec();
    let feeder = thread::spawn(move || input_writer.write_all(&input));
    let started = Instant::now();

    loop {
        if child.try_wait().expect("poll pack stdin child").is_some() {
            let output = child
                .wait_with_output()
                .expect("collect pack stdin child output");
            let _ = feeder.join();
            return Ok(output);
        }
        if started.elapsed() >= CHILD_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = feeder.join();
            return Err(format!("pack stdin child exceeded {CHILD_TIMEOUT:?}"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn run_bounded_with_fifo<I, S>(args: I, fifo: &Path, source: &Path) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    // Start the reader first so a rejected implementation cannot block the
    // writer before the child-process timeout is active.
    let mut child = pack_command()
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pack FIFO child");
    let mut writer = Command::new("cp")
        .arg(source)
        .arg(fifo)
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn FIFO writer");
    let started = Instant::now();

    let output = loop {
        if child.try_wait().expect("poll pack FIFO child").is_some() {
            break Ok(child
                .wait_with_output()
                .expect("collect pack FIFO child output"));
        }
        if started.elapsed() >= CHILD_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = writer.kill();
            let _ = writer.wait();
            break Err(format!("pack FIFO child exceeded {CHILD_TIMEOUT:?}"));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let _ = writer.kill();
    let _ = writer.wait();
    output
}

fn assert_refusal(output: Output, expected_code: &str) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout={stdout}\nstderr={stderr}"
    );
    assert!(stderr.trim().is_empty(), "unexpected stderr: {stderr}");
    let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);
    assert!(parsed.is_ok(), "refusal stdout is not JSON: {stdout}");
    let envelope = parsed.unwrap_or(Value::Null);
    assert_eq!(envelope["outcome"], "REFUSAL");
    assert_eq!(envelope["refusal"]["code"], expected_code);
    envelope
}

fn assert_import_success(output: &Output, destination: &Path) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={:?}\nstderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).starts_with("ARCHIVE_IMPORTED sha256:"),
        "stdout={:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(destination.join("manifest.json").is_file());
}

fn assert_no_output(path: &Path) {
    if path.exists() {
        assert!(path.is_dir(), "unexpected output file: {}", path.display());
        assert_eq!(
            fs::read_dir(path).unwrap().count(),
            0,
            "partial output: {}",
            path.display()
        );
    }
}

fn make_pack() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<u8>, Vec<u8>) {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("payload.json");
    let content = vec![b'x'; 4096];
    fs::write(&source, &content).unwrap();
    let pack_dir = tmp.path().join("pack");
    let seal = run_bounded([
        "seal",
        source.to_str().unwrap(),
        "--out",
        pack_dir.to_str().unwrap(),
        "--created",
        "2026-01-15T00:00:00Z",
        "--no-witness",
    ]);
    assert!(seal.status.success(), "seal failed: {:?}", seal);

    let archive = tmp.path().join("pack.tar");
    let exported = run_bounded([
        "archive",
        "export",
        pack_dir.to_str().unwrap(),
        "--out",
        archive.to_str().unwrap(),
        "--no-witness",
    ]);
    assert!(
        exported.status.success(),
        "archive export failed: {:?}",
        exported
    );

    let manifest = fs::read(pack_dir.join("manifest.json")).unwrap();
    (tmp, pack_dir, archive, manifest, content)
}

fn run_import(archive: &Path, output: &Path) -> Output {
    run_bounded([
        "--no-witness".to_string(),
        "archive".to_string(),
        "import".to_string(),
        archive.display().to_string(),
        "--out".to_string(),
        output.display().to_string(),
    ])
}

fn tar_archive(entries: &[TarEntry<'_>]) -> Vec<u8> {
    let mut archive = Vec::new();
    for entry in entries {
        write_tar_entry(&mut archive, *entry);
    }
    archive.extend_from_slice(&[0u8; TAR_BLOCK_SIZE * 2]);
    archive
}

fn write_tar_entry(archive: &mut Vec<u8>, entry: TarEntry<'_>) {
    let mut header = [0u8; TAR_BLOCK_SIZE];
    assert!(entry.path.len() <= 100);
    header[..entry.path.len()].copy_from_slice(entry.path.as_bytes());
    write_octal(&mut header[100..108], 0o644);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(&mut header[124..136], entry.bytes.len() as u64);
    write_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = entry.typeflag;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    assert!(entry.link_name.len() <= 100);
    header[157..157 + entry.link_name.len()].copy_from_slice(entry.link_name.as_bytes());
    let checksum = header.iter().map(|byte| u64::from(*byte)).sum::<u64>();
    write_checksum(&mut header[148..156], checksum);
    archive.extend_from_slice(&header);
    archive.extend_from_slice(entry.bytes);
    let padding = (TAR_BLOCK_SIZE - (entry.bytes.len() % TAR_BLOCK_SIZE)) % TAR_BLOCK_SIZE;
    archive.extend(std::iter::repeat_n(0, padding));
}

fn write_octal(field: &mut [u8], value: u64) {
    let text = format!("{value:0width$o}\0", width = field.len() - 1);
    field.copy_from_slice(text.as_bytes());
}

fn write_checksum(field: &mut [u8], value: u64) {
    let text = format!("{value:06o}\0 ");
    field.copy_from_slice(text.as_bytes());
}

fn archive_with_pack_members(manifest: &[u8], member: &[u8]) -> Vec<u8> {
    tar_archive(&[
        TarEntry {
            path: "manifest.json",
            typeflag: b'0',
            bytes: manifest,
            link_name: "",
        },
        TarEntry {
            path: "payload.json",
            typeflag: b'0',
            bytes: member,
            link_name: "",
        },
    ])
}

#[test]
fn archive_adversarial_matrix_is_typed_and_atomic() {
    let (tmp, _pack_dir, archive, manifest, content) = make_pack();
    let valid_archive = fs::read(&archive).unwrap();

    let run_case = |name: &str, bytes: Vec<u8>, expected_code: &str| {
        let archive_path = tmp.path().join(format!("{name}.tar"));
        let output_dir = tmp.path().join(format!("out-{name}"));
        fs::write(&archive_path, bytes).unwrap();
        let output = run_import(&archive_path, &output_dir);
        let _ = assert_refusal(output, expected_code);
        assert_no_output(&output_dir);
    };

    let duplicate = tar_archive(&[
        TarEntry {
            path: "manifest.json",
            typeflag: b'0',
            bytes: &manifest,
            link_name: "",
        },
        TarEntry {
            path: "payload.json",
            typeflag: b'0',
            bytes: &content,
            link_name: "",
        },
        TarEntry {
            path: "payload.json",
            typeflag: b'0',
            bytes: &content,
            link_name: "",
        },
    ]);
    run_case("duplicate", duplicate, "E_BAD_PACK");

    let curdir = tar_archive(&[
        TarEntry {
            path: "manifest.json",
            typeflag: b'0',
            bytes: &manifest,
            link_name: "",
        },
        TarEntry {
            path: "./manifest.json",
            typeflag: b'0',
            bytes: b"overwrite",
            link_name: "",
        },
    ]);
    run_case("curdir", curdir, "E_BAD_PACK");

    for (name, path) in [
        ("traversal", "../escape.txt"),
        ("absolute", "/tmp/escape.txt"),
    ] {
        let unsafe_path = tar_archive(&[TarEntry {
            path,
            typeflag: b'0',
            bytes: b"escape",
            link_name: "",
        }]);
        run_case(name, unsafe_path, "E_BAD_PACK");
    }

    for (name, typeflag, link_name) in [
        ("file-symlink", b'2', "../outside/secret"),
        ("directory-symlink", b'5', "../outside"),
    ] {
        let symlink = tar_archive(&[
            TarEntry {
                path: "manifest.json",
                typeflag: b'0',
                bytes: &manifest,
                link_name: "",
            },
            TarEntry {
                path: "outside-link",
                typeflag,
                bytes: b"",
                link_name,
            },
        ]);
        run_case(name, symlink, "E_BAD_PACK");
    }

    let mut tampered = content.clone();
    tampered[2048] ^= 1;
    run_case(
        "stale-hash",
        archive_with_pack_members(&manifest, &tampered),
        "E_BAD_PACK",
    );

    for (name, prefix) in [("unknown-prefix", "md5:"), ("blake3-prefix", "blake3:")] {
        let mut value: Value = serde_json::from_slice(&manifest).unwrap();
        value["members"][0]["bytes_hash"] = json!(format!("{prefix}{}", "0".repeat(64)));
        let changed_manifest = serde_json::to_vec(&value).unwrap();
        run_case(
            name,
            archive_with_pack_members(&changed_manifest, &content),
            "E_BAD_PACK",
        );
    }

    let manifest_header_end = TAR_BLOCK_SIZE;
    let member_header =
        manifest_header_end + manifest.len().div_ceil(TAR_BLOCK_SIZE) * TAR_BLOCK_SIZE;
    let mid_member = member_header + TAR_BLOCK_SIZE + content.len() / 2;
    for (name, cut) in [
        ("truncated-header", 1),
        ("truncated-header-end", TAR_BLOCK_SIZE - 1),
        ("truncated-mid-member", mid_member),
        (
            "truncated-final-padding",
            valid_archive.len() - TAR_BLOCK_SIZE,
        ),
    ] {
        assert!(cut < valid_archive.len());
        run_case(name, valid_archive[..cut].to_vec(), "E_BAD_PACK");
    }
}

#[cfg(unix)]
#[test]
fn unreadable_inputs_are_typed_and_leave_no_output() {
    use std::os::unix::fs::PermissionsExt;

    let (tmp, pack_dir, archive, _manifest, _content) = make_pack();

    let member = pack_dir.join("payload.json");
    let mut member_permissions = fs::metadata(&member).unwrap().permissions();
    member_permissions.set_mode(0o0);
    fs::set_permissions(&member, member_permissions).unwrap();
    let verified = run_bounded([
        "--no-witness",
        "verify",
        pack_dir.to_str().unwrap(),
        "--json",
    ]);
    let _ = assert_refusal(verified, "E_IO");
    let mut restore_member = fs::metadata(&member).unwrap().permissions();
    restore_member.set_mode(0o644);
    fs::set_permissions(&member, restore_member).unwrap();

    let source = tmp.path().join("unreadable-input.json");
    fs::write(&source, b"input").unwrap();
    let mut source_permissions = fs::metadata(&source).unwrap().permissions();
    source_permissions.set_mode(0o0);
    fs::set_permissions(&source, source_permissions).unwrap();
    let seal_output = tmp.path().join("unreadable-seal-output");
    let sealed = run_bounded([
        "--no-witness".to_string(),
        "seal".to_string(),
        source.display().to_string(),
        "--out".to_string(),
        seal_output.display().to_string(),
        "--created".to_string(),
        "2026-01-15T00:00:00Z".to_string(),
    ]);
    let _ = assert_refusal(sealed, "E_IO");
    assert_no_output(&seal_output);
    let mut restore_source = fs::metadata(&source).unwrap().permissions();
    restore_source.set_mode(0o644);
    fs::set_permissions(&source, restore_source).unwrap();

    let mut archive_permissions = fs::metadata(&archive).unwrap().permissions();
    archive_permissions.set_mode(0o0);
    fs::set_permissions(&archive, archive_permissions).unwrap();
    let import_output = tmp.path().join("unreadable-import-output");
    let imported = run_import(&archive, &import_output);
    let _ = assert_refusal(imported, "E_IO");
    assert_no_output(&import_output);
    let mut restore_archive = fs::metadata(&archive).unwrap().permissions();
    restore_archive.set_mode(0o644);
    fs::set_permissions(&archive, restore_archive).unwrap();
}

#[test]
fn directory_where_archive_file_is_expected_is_typed_and_atomic() {
    let tmp = tempfile::tempdir().unwrap();
    let archive_directory = tmp.path().join("archive-directory");
    fs::create_dir(&archive_directory).unwrap();
    let output_dir = tmp.path().join("imported");
    let output = run_import(&archive_directory, &output_dir);
    let _ = assert_refusal(output, "E_IO");
    assert_no_output(&output_dir);
}

#[cfg(unix)]
#[test]
fn archive_input_rejects_only_directories_and_follows_streams() {
    use std::os::unix::fs::symlink;

    let (tmp, _pack_dir, archive, _manifest, _content) = make_pack();
    let archive_bytes = fs::read(&archive).unwrap();

    // A symlink to a regular archive must remain an accepted input.
    let archive_link = tmp.path().join("archive-link.tar");
    symlink(&archive, &archive_link).unwrap();
    let linked_destination = tmp.path().join("linked-import");
    let linked = run_import(&archive_link, &linked_destination);
    assert_import_success(&linked, &linked_destination);

    // A FIFO is a valid stream input. Keep the writer in a separately
    // killable child so an implementation that rejects before opening it
    // cannot strand this test on a blocked FIFO open.
    let fifo = tmp.path().join("archive.fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success(),
        "mkfifo failed"
    );
    let fifo_destination = tmp.path().join("fifo-import");
    let fifo_result = run_bounded_with_fifo(
        [
            "--no-witness".to_owned(),
            "archive".to_owned(),
            "import".to_owned(),
            fifo.display().to_string(),
            "--out".to_owned(),
            fifo_destination.display().to_string(),
        ],
        &fifo,
        &archive,
    )
    .expect("pack FIFO child");
    assert_import_success(&fifo_result, &fifo_destination);

    // /dev/stdin is also an accepted stream input.
    let stdin_destination = tmp.path().join("stdin-import");
    let stdin_result = run_bounded_with_stdin(
        [
            "--no-witness".to_owned(),
            "archive".to_owned(),
            "import".to_owned(),
            "/dev/stdin".to_owned(),
            "--out".to_owned(),
            stdin_destination.display().to_string(),
        ],
        &archive_bytes,
    )
    .expect("pack stdin child");
    assert_import_success(&stdin_result, &stdin_destination);

    // Directory symlinks are followed and refused before staging/extraction.
    let directory = tmp.path().join("archive-directory");
    fs::create_dir(&directory).unwrap();
    let directory_link = tmp.path().join("directory-link");
    symlink(&directory, &directory_link).unwrap();
    let directory_destination = tmp.path().join("directory-import");
    let directory_result = run_import(&directory_link, &directory_destination);
    let _ = assert_refusal(directory_result, "E_IO");
    assert_no_output(&directory_destination);
}
