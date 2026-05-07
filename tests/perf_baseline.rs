use serde::Serialize;
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const FIXED_CREATED: &str = "2026-01-15T10:30:00Z";

#[derive(Debug, Clone)]
struct ScenarioConfig {
    name: String,
    file_count: usize,
    bytes_per_file: usize,
}

impl ScenarioConfig {
    fn total_bytes(&self) -> u64 {
        (self.file_count as u64) * (self.bytes_per_file as u64)
    }
}

#[derive(Debug, Serialize)]
struct BaselineReport {
    schema: &'static str,
    binary: String,
    measurement: MeasurementReport,
    scenarios: Vec<ScenarioReport>,
}

#[derive(Debug, Serialize)]
struct MeasurementReport {
    time_source: &'static str,
    memory_source: String,
}

#[derive(Debug, Serialize)]
struct ScenarioReport {
    name: String,
    file_count: usize,
    bytes_per_file: usize,
    total_bytes: u64,
    determinism: DeterminismReport,
    operations: Vec<OperationReport>,
}

#[derive(Debug, Serialize)]
struct DeterminismReport {
    same_manifest_bytes: bool,
    same_pack_id: bool,
    changed_pack_id_differs: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct OperationReport {
    name: String,
    exit_code: i32,
    duration_ms: u64,
    throughput_mib_s: Option<f64>,
    max_rss_kib: Option<u64>,
}

struct OperationRun {
    report: OperationReport,
    stdout: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
enum TimeMode {
    Bsd,
    Gnu,
}

/// Safe CI smoke: report serialization stays deterministic and includes the
/// fields that the ignored baseline emits for real runs.
#[test]
fn perf_report_shape_is_stable() {
    let report = BaselineReport {
        schema: "pack.perf-baseline.v0",
        binary: "target/release/pack".to_string(),
        measurement: MeasurementReport {
            time_source: "std::time::Instant",
            memory_source: "/usr/bin/time unavailable".to_string(),
        },
        scenarios: vec![ScenarioReport {
            name: "many_small_files".to_string(),
            file_count: 2,
            bytes_per_file: 128,
            total_bytes: 256,
            determinism: DeterminismReport {
                same_manifest_bytes: true,
                same_pack_id: true,
                changed_pack_id_differs: true,
            },
            operations: vec![OperationReport {
                name: "seal_original".to_string(),
                exit_code: 0,
                duration_ms: 12,
                throughput_mib_s: Some(20.833),
                max_rss_kib: None,
            }],
        }],
    };

    let first = serde_json::to_string_pretty(&report).unwrap();
    let second = serde_json::to_string_pretty(&report).unwrap();
    assert_eq!(first, second);
    assert!(first.contains("\"schema\": \"pack.perf-baseline.v0\""));
    assert!(first.contains("\"throughput_mib_s\""));
    assert!(first.contains("\"max_rss_kib\""));
}

/// Ignored by default because it creates large temporary packs and measures
/// local machine performance. Run with:
///
/// cargo build --release
/// PACK_PERF_BIN=target/release/pack cargo test --test perf_baseline -- --ignored --nocapture --test-threads=1
#[test]
#[ignore = "local performance baseline; use --ignored --nocapture"]
fn large_pack_performance_baseline() {
    let binary = pack_binary();
    let scenarios = vec![
        ScenarioConfig {
            name: "many_small_files".to_string(),
            file_count: env_usize("PACK_PERF_SMALL_FILES", 1_000),
            bytes_per_file: env_usize("PACK_PERF_SMALL_BYTES", 1_024),
        },
        ScenarioConfig {
            name: "few_large_files".to_string(),
            file_count: env_usize("PACK_PERF_LARGE_FILES", 8),
            bytes_per_file: env_usize("PACK_PERF_LARGE_BYTES", 2 * 1_024 * 1_024),
        },
    ];

    let tmp = tempfile::tempdir().unwrap();
    let mut scenario_reports = Vec::new();
    for scenario in scenarios {
        scenario_reports.push(run_scenario(tmp.path(), &binary, &scenario));
    }

    let memory_source = match time_mode() {
        Some(TimeMode::Bsd) => "/usr/bin/time -l maximum resident set size".to_string(),
        Some(TimeMode::Gnu) => "/usr/bin/time -v Maximum resident set size".to_string(),
        None => "/usr/bin/time unavailable; max_rss_kib is null".to_string(),
    };

    let report = BaselineReport {
        schema: "pack.perf-baseline.v0",
        binary,
        measurement: MeasurementReport {
            time_source: "std::time::Instant",
            memory_source,
        },
        scenarios: scenario_reports,
    };

    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn run_scenario(base: &Path, binary: &str, scenario: &ScenarioConfig) -> ScenarioReport {
    let scenario_dir = base.join(&scenario.name);
    let original_source = scenario_dir.join("original").join("artifacts");
    let changed_source = scenario_dir.join("changed").join("artifacts");
    let pack_a = scenario_dir.join("pack-a");
    let pack_a_repeat = scenario_dir.join("pack-a-repeat");
    let pack_b = scenario_dir.join("pack-b");

    let original_bytes = write_dataset(&original_source, scenario, false);
    let changed_bytes = write_dataset(&changed_source, scenario, true);
    assert_eq!(original_bytes, scenario.total_bytes());
    assert_eq!(changed_bytes, scenario.total_bytes());

    let seal_original = run_pack(
        binary,
        "seal_original",
        vec![
            os("--no-witness"),
            os("seal"),
            os_path(&original_source),
            os("--output"),
            os_path(&pack_a),
            os("--created"),
            os(FIXED_CREATED),
        ],
        0,
        Some(original_bytes),
    );
    let seal_repeat = run_pack(
        binary,
        "seal_repeat",
        vec![
            os("--no-witness"),
            os("seal"),
            os_path(&original_source),
            os("--output"),
            os_path(&pack_a_repeat),
            os("--created"),
            os(FIXED_CREATED),
        ],
        0,
        Some(original_bytes),
    );
    let verify_original = run_pack(
        binary,
        "verify_original",
        vec![
            os("--no-witness"),
            os("verify"),
            os_path(&pack_a),
            os("--json"),
        ],
        0,
        Some(original_bytes),
    );
    let seal_changed = run_pack(
        binary,
        "seal_changed",
        vec![
            os("--no-witness"),
            os("seal"),
            os_path(&changed_source),
            os("--output"),
            os_path(&pack_b),
            os("--created"),
            os(FIXED_CREATED),
        ],
        0,
        Some(changed_bytes),
    );
    let diff_identical = run_pack(
        binary,
        "diff_identical",
        vec![
            os("--no-witness"),
            os("diff"),
            os_path(&pack_a),
            os_path(&pack_a_repeat),
            os("--json"),
        ],
        0,
        None,
    );
    let diff_changed = run_pack(
        binary,
        "diff_changed",
        vec![
            os("--no-witness"),
            os("diff"),
            os_path(&pack_a),
            os_path(&pack_b),
            os("--json"),
        ],
        1,
        None,
    );

    let manifest_a = read_manifest(&pack_a);
    let manifest_a_repeat = read_manifest(&pack_a_repeat);
    let manifest_b = read_manifest(&pack_b);
    let pack_id_a = manifest_a["pack_id"].as_str().unwrap();
    let pack_id_a_repeat = manifest_a_repeat["pack_id"].as_str().unwrap();
    let pack_id_b = manifest_b["pack_id"].as_str().unwrap();
    let manifest_bytes_a = std::fs::read(pack_a.join("manifest.json")).unwrap();
    let manifest_bytes_a_repeat = std::fs::read(pack_a_repeat.join("manifest.json")).unwrap();

    assert_eq!(pack_id_a, pack_id_a_repeat);
    assert_ne!(pack_id_a, pack_id_b);
    assert_eq!(manifest_bytes_a, manifest_bytes_a_repeat);
    assert_eq!(manifest_a["member_count"], scenario.file_count);

    let verify_json: Value = serde_json::from_slice(&verify_original.stdout).unwrap();
    assert_eq!(verify_json["outcome"], "OK");
    let diff_same_json: Value = serde_json::from_slice(&diff_identical.stdout).unwrap();
    assert_eq!(diff_same_json["outcome"], "NO_CHANGES");
    let diff_changed_json: Value = serde_json::from_slice(&diff_changed.stdout).unwrap();
    assert_eq!(diff_changed_json["outcome"], "CHANGES");

    ScenarioReport {
        name: scenario.name.clone(),
        file_count: scenario.file_count,
        bytes_per_file: scenario.bytes_per_file,
        total_bytes: scenario.total_bytes(),
        determinism: DeterminismReport {
            same_manifest_bytes: manifest_bytes_a == manifest_bytes_a_repeat,
            same_pack_id: pack_id_a == pack_id_a_repeat,
            changed_pack_id_differs: pack_id_a != pack_id_b,
        },
        operations: vec![
            seal_original.report,
            seal_repeat.report,
            verify_original.report,
            seal_changed.report,
            diff_identical.report,
            diff_changed.report,
        ],
    }
}

fn run_pack(
    binary: &str,
    name: &str,
    args: Vec<OsString>,
    expected_code: i32,
    throughput_bytes: Option<u64>,
) -> OperationRun {
    let time_mode = time_mode();
    let start = Instant::now();
    let output = run_command(binary, &args, time_mode);
    let duration = start.elapsed();
    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code,
        expected_code,
        "{name} exited unexpectedly\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    OperationRun {
        report: OperationReport {
            name: name.to_string(),
            exit_code,
            duration_ms: duration_ms(duration),
            throughput_mib_s: throughput_bytes.and_then(|bytes| throughput_mib_s(bytes, duration)),
            max_rss_kib: parse_max_rss_kib(&String::from_utf8_lossy(&output.stderr), time_mode),
        },
        stdout: output.stdout,
    }
}

fn run_command(binary: &str, args: &[OsString], time_mode: Option<TimeMode>) -> Output {
    if let Some(mode) = time_mode {
        let mut command = Command::new("/usr/bin/time");
        match mode {
            TimeMode::Bsd => {
                command.arg("-l");
            }
            TimeMode::Gnu => {
                command.arg("-v");
            }
        }
        return command.arg(binary).args(args).output().unwrap();
    }

    Command::new(binary).args(args).output().unwrap()
}

fn time_mode() -> Option<TimeMode> {
    if std::env::var("PACK_PERF_RSS").as_deref() == Ok("0") {
        return None;
    }
    if !Path::new("/usr/bin/time").exists() {
        return None;
    }
    if cfg!(target_os = "macos") {
        Some(TimeMode::Bsd)
    } else if cfg!(target_os = "linux") {
        Some(TimeMode::Gnu)
    } else {
        None
    }
}

fn parse_max_rss_kib(stderr: &str, time_mode: Option<TimeMode>) -> Option<u64> {
    match time_mode? {
        TimeMode::Bsd => stderr
            .lines()
            .find(|line| line.contains("maximum resident set size"))
            .and_then(|line| line.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|bytes| bytes.div_ceil(1_024)),
        TimeMode::Gnu => stderr.lines().find_map(|line| {
            line.trim()
                .strip_prefix("Maximum resident set size (kbytes):")
                .and_then(|value| value.trim().parse::<u64>().ok())
        }),
    }
}

fn write_dataset(root: &Path, scenario: &ScenarioConfig, changed: bool) -> u64 {
    std::fs::create_dir_all(root).unwrap();
    let mut total = 0;
    for index in 0..scenario.file_count {
        let member = root.join(format!("member_{index:06}.bin"));
        let bytes = deterministic_bytes(scenario.bytes_per_file, index, changed && index == 0);
        total += bytes.len() as u64;
        std::fs::write(member, bytes).unwrap();
    }
    total
}

fn deterministic_bytes(len: usize, index: usize, changed: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len);
    for offset in 0..len {
        let value = ((index as u64 * 31 + offset as u64 * 17) % 251) as u8;
        bytes.push(value);
    }
    if changed && !bytes.is_empty() {
        bytes[0] = bytes[0].wrapping_add(1);
    }
    bytes
}

fn read_manifest(pack_dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(pack_dir.join("manifest.json")).unwrap()).unwrap()
}

fn throughput_mib_s(bytes: u64, duration: Duration) -> Option<f64> {
    let seconds = duration.as_secs_f64();
    if seconds == 0.0 || bytes == 0 {
        return None;
    }
    Some(round_three((bytes as f64 / 1_048_576.0) / seconds))
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn round_three(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

fn pack_binary() -> String {
    std::env::var("PACK_PERF_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_pack").to_string())
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn os(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_os_string()
}

fn os_path(path: &Path) -> OsString {
    path.as_os_str().to_os_string()
}
