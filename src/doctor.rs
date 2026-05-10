use crate::cli::{DoctorAction, ExitCode};
use serde_json::{json, Value};
use std::path::Path;

const HEALTH_SCHEMA_VERSION: &str = "pack.doctor.health.v1";
const CAPABILITIES_SCHEMA_VERSION: &str = "pack.doctor.capabilities.v1";
const TRIAGE_SCHEMA_VERSION: &str = "pack.doctor.triage.v1";
const READ_ONLY_DOCTOR_CONTRACT: &str = "cmdrvl.read_only_doctor.v1";
const OPERATOR_JSON: &str = include_str!("../operator.json");

pub fn dispatch(robot_triage: bool, json_mode: bool, action: Option<&DoctorAction>) -> u8 {
    if robot_triage {
        println!("{}", json_string(&triage_report()));
        return ExitCode::Success.into();
    }

    match action {
        Some(DoctorAction::Health { json }) => print_health(*json),
        Some(DoctorAction::Capabilities { json }) => print_capabilities(*json),
        Some(DoctorAction::RobotDocs) => {
            println!("{}", robot_docs());
            ExitCode::Success.into()
        }
        None => print_health(json_mode),
    }
}

fn print_health(json_mode: bool) -> u8 {
    let report = health_report();
    if json_mode {
        println!("{}", json_string(&report));
    } else {
        println!("{}", human_health(&report));
    }
    ExitCode::Success.into()
}

fn print_capabilities(json_mode: bool) -> u8 {
    let report = capabilities_report();
    if json_mode {
        println!("{}", json_string(&report));
    } else {
        println!("{}", human_capabilities(&report));
    }
    ExitCode::Success.into()
}

fn triage_report() -> Value {
    let health = health_report();
    let capabilities = capabilities_report();
    let ok = health.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let recommended_actions = health
        .get("recommended_actions")
        .cloned()
        .unwrap_or_else(|| json!([]));

    json!({
        "schema_version": TRIAGE_SCHEMA_VERSION,
        "contract": READ_ONLY_DOCTOR_CONTRACT,
        "tool": "pack",
        "version": env!("CARGO_PKG_VERSION"),
        "ok": ok,
        "health": health,
        "capabilities": capabilities,
        "recommended_actions": recommended_actions
    })
}

fn health_report() -> Value {
    let checks = vec![
        check_operator_manifest(),
        check_pack_schema(),
        check_witness_path_resolution(),
        check_artifact_command_contract(),
        check_transport_contract(),
    ];
    let summary = summarize(&checks);
    let ok = summary.error == 0;
    let recommended_actions = if ok {
        json!([])
    } else {
        json!(["Inspect failed doctor checks before using pack in automation."])
    };

    json!({
        "schema_version": HEALTH_SCHEMA_VERSION,
        "contract": READ_ONLY_DOCTOR_CONTRACT,
        "tool": "pack",
        "version": env!("CARGO_PKG_VERSION"),
        "ok": ok,
        "read_only": true,
        "summary": {
            "total": summary.total,
            "ok": summary.ok,
            "warn": summary.warn,
            "error": summary.error
        },
        "checks": checks,
        "fixers": [],
        "recommended_actions": recommended_actions
    })
}

fn capabilities_report() -> Value {
    json!({
        "schema_version": CAPABILITIES_SCHEMA_VERSION,
        "contract": READ_ONLY_DOCTOR_CONTRACT,
        "tool": "pack",
        "version": env!("CARGO_PKG_VERSION"),
        "read_only": true,
        "commands": [
            {
                "command": "pack doctor health",
                "json": "pack doctor health --json",
                "description": "Run read-only static health checks."
            },
            {
                "command": "pack doctor capabilities --json",
                "description": "Describe the doctor command surface and mutation policy."
            },
            {
                "command": "pack doctor robot-docs",
                "description": "Print agent-oriented usage notes."
            },
            {
                "command": "pack doctor --robot-triage",
                "description": "Emit health and capabilities in one robot-readable report."
            }
        ],
        "detectors": [
            {"name": "operator_manifest", "mode": "compiled_static_json", "mutates": false},
            {"name": "pack_schema", "mode": "compiled_static_json", "mutates": false},
            {"name": "witness_path_resolution", "mode": "environment_resolution_only", "mutates": false},
            {"name": "artifact_command_contract", "mode": "static_contract", "mutates": false},
            {"name": "transport_contract", "mode": "static_contract", "mutates": false}
        ],
        "side_effects": {
            "reads_pack_inputs": false,
            "reads_pack_members": false,
            "walks_pack_dirs": false,
            "seals_packs": false,
            "verifies_pack_integrity": false,
            "diffs_pack_dirs": false,
            "exports_archives": false,
            "imports_archives": false,
            "writes_witness_ledger": false,
            "creates_witness_directory": false,
            "writes_doctor_artifacts": false,
            "rewrites_operator_manifest": false,
            "rewrites_schema": false,
            "uses_network": false
        },
        "network": {
            "required": false,
            "used": false,
            "config_env_read": false
        },
        "output_contract": {
            "doctor_stdout": "human text or JSON doctor reports",
            "doctor_stderr": "unused on successful doctor commands",
            "seal_stdout": "PACK_CREATED status lines or REFUSAL envelope",
            "verify_stdout": "human text or pack.verify.v0 JSON depending on --json",
            "inspect_stdout": "human text or pack.inspect.v0 JSON depending on --json",
            "diff_stdout": "human text or pack.diff.v0 JSON depending on --json",
            "archive_stdout": "status lines or REFUSAL envelopes"
        },
        "fix_mode": {
            "status": "not_available",
            "command": null,
            "reason": "No pack fixer has detector, backup, inverse, and fixture coverage yet."
        },
        "fixers": []
    })
}

fn check_operator_manifest() -> Value {
    match serde_json::from_str::<Value>(OPERATOR_JSON) {
        Ok(value) => {
            let schema_version = string_field(&value, "schema_version");
            let name = string_field(&value, "name");
            let version = string_field(&value, "version");

            if schema_version == Some("operator.v0")
                && name == Some("pack")
                && version == Some(env!("CARGO_PKG_VERSION"))
            {
                check(
                    "operator_manifest",
                    "ok",
                    "Compiled operator manifest matches the current binary.",
                    json!({
                        "schema_version": schema_version,
                        "version": version
                    }),
                )
            } else {
                check(
                    "operator_manifest",
                    "error",
                    "Compiled operator manifest does not match the current binary.",
                    json!({
                        "schema_version": schema_version,
                        "name": name,
                        "version": version,
                        "expected_version": env!("CARGO_PKG_VERSION")
                    }),
                )
            }
        }
        Err(error) => check(
            "operator_manifest",
            "error",
            "Compiled operator manifest is not valid JSON.",
            json!({ "error": error.to_string() }),
        ),
    }
}

fn check_pack_schema() -> Value {
    let schema = crate::schema::pack_schema();
    let id = string_field(&schema, "$id");
    let title = string_field(&schema, "title");
    let definitions = schema.get("definitions").and_then(Value::as_object);
    let has_manifest = definitions
        .map(|defs| defs.contains_key("manifest"))
        .unwrap_or(false);
    let has_verify_report = definitions
        .map(|defs| defs.contains_key("verify_report"))
        .unwrap_or(false);

    if id == Some("pack.v0")
        && title == Some("pack.v0 manifest and verify schema")
        && has_manifest
        && has_verify_report
    {
        check(
            "pack_schema",
            "ok",
            "Compiled schema advertises pack.v0 manifest and verify report definitions.",
            json!({
                "id": id,
                "title": title,
                "has_manifest_definition": has_manifest,
                "has_verify_report_definition": has_verify_report
            }),
        )
    } else {
        check(
            "pack_schema",
            "error",
            "Compiled schema is missing expected pack.v0 metadata.",
            json!({
                "id": id,
                "title": title,
                "has_manifest_definition": has_manifest,
                "has_verify_report_definition": has_verify_report
            }),
        )
    }
}

fn check_witness_path_resolution() -> Value {
    let path = crate::witness::witness_ledger_path();
    let parent = path.parent().map(Path::to_path_buf);
    let parent_exists = parent.as_deref().map(Path::exists).unwrap_or(false);

    check(
        "witness_path_resolution",
        "ok",
        "Resolved witness ledger path without creating directories or appending records.",
        json!({
            "path": path.display().to_string(),
            "parent": parent
                .as_ref()
                .map(|value| value.display().to_string()),
            "parent_exists": parent_exists,
            "write_attempted": false
        }),
    )
}

fn check_artifact_command_contract() -> Value {
    check(
        "artifact_command_contract",
        "ok",
        "Doctor commands are outside seal, verify, inspect, diff, and archive paths.",
        json!({
            "seal": false,
            "verify": false,
            "inspect": false,
            "diff": false,
            "archive": false,
            "witness_append": false
        }),
    )
}

fn check_transport_contract() -> Value {
    check(
        "transport_contract",
        "ok",
        "Doctor commands do not read network configuration or open network connections.",
        json!({
            "uses_network": false,
            "reads_network_env": false
        }),
    )
}

fn check(name: &str, status: &str, message: &str, details: Value) -> Value {
    json!({
        "name": name,
        "status": status,
        "message": message,
        "details": details
    })
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[derive(Default)]
struct Summary {
    total: usize,
    ok: usize,
    warn: usize,
    error: usize,
}

fn summarize(checks: &[Value]) -> Summary {
    let mut summary = Summary {
        total: checks.len(),
        ..Summary::default()
    };

    for check in checks {
        match string_field(check, "status") {
            Some("ok") => summary.ok += 1,
            Some("warn") => summary.warn += 1,
            Some("error") => summary.error += 1,
            _ => summary.warn += 1,
        }
    }

    summary
}

fn human_health(report: &Value) -> String {
    let summary = report.get("summary").unwrap_or(&Value::Null);
    let ok = summary.get("ok").and_then(Value::as_u64).unwrap_or(0);
    let warn = summary.get("warn").and_then(Value::as_u64).unwrap_or(0);
    let error = summary.get("error").and_then(Value::as_u64).unwrap_or(0);
    let total = summary.get("total").and_then(Value::as_u64).unwrap_or(0);
    let status = if error == 0 { "healthy" } else { "unhealthy" };
    let mut lines = vec![format!(
        "pack doctor {status}: {ok} checks passed, {warn} warnings, {error} errors"
    )];

    if let Some(checks) = report.get("checks").and_then(Value::as_array) {
        for check in checks {
            let check_status = string_field(check, "status").unwrap_or("warn");
            let marker = match check_status {
                "ok" => "OK",
                "warn" => "WARN",
                "error" => "ERROR",
                _ => "WARN",
            };
            let name = string_field(check, "name").unwrap_or("unknown");
            let message = string_field(check, "message").unwrap_or("");
            lines.push(format!("[{marker}] {name}: {message}"));
        }
    }

    if total == 0 {
        lines.push("[WARN] no_checks: No doctor checks were executed.".to_string());
    }

    lines.join("\n")
}

fn human_capabilities(report: &Value) -> String {
    let read_only = report
        .get("read_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let fix_status = report
        .get("fix_mode")
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    format!("pack doctor capabilities: read_only={read_only} fix_mode={fix_status}")
}

fn robot_docs() -> String {
    [
        "# pack doctor robot docs",
        "",
        "`pack doctor` is a read-only diagnostic surface for agents.",
        "It does not read pack inputs or members, walk pack directories, seal packs, verify integrity, diff directories, import or export archives, append witness records, create witness directories, write doctor artifacts, rewrite metadata, or use the network.",
        "",
        "Commands:",
        "- `pack doctor health` for human health output.",
        "- `pack doctor health --json` for machine-readable health.",
        "- `pack doctor capabilities --json` for command and side-effect policy.",
        "- `pack doctor --robot-triage` for a single JSON triage payload.",
        "",
        "No fix mode is available. `pack doctor --fix` is intentionally unsupported.",
        "Use `pack verify` when you need to verify pack integrity; doctor does not verify pack contents.",
    ]
    .join("\n")
}

fn json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| {
        format!(
            "{{\"schema_version\":\"{TRIAGE_SCHEMA_VERSION}\",\"tool\":\"pack\",\"ok\":false,\"serialization_error\":\"{error}\"}}"
        )
    })
}
