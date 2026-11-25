use crate::cli::{DoctorAction, ExitCode, RobotDocsAction};
use serde_json::{json, Value};
use std::path::Path;

const DOCTOR_SCHEMA_VERSION: &str = "cmdrvl.doctor.v1";
const OPERATOR_JSON: &str = include_str!("../operator.json");

pub fn dispatch(
    robot_triage: bool,
    fix: bool,
    json_mode: bool,
    action: Option<&DoctorAction>,
) -> u8 {
    if fix {
        return fix_unavailable();
    }
    if robot_triage {
        return dispatch_robot_triage();
    }

    match action {
        Some(DoctorAction::Health { json }) => print_health(*json || json_mode),
        Some(DoctorAction::Capabilities { json }) => print_doctor_capabilities(*json || json_mode),
        Some(DoctorAction::Triage { json }) => print_triage(*json || json_mode),
        Some(DoctorAction::RobotDocs) => dispatch_robot_docs(None),
        None => print_health(json_mode),
    }
}

/// `capabilities --json` is the canonical operator contract and must match
/// `--describe` exactly.
pub fn dispatch_capabilities(json_mode: bool) -> u8 {
    if json_mode {
        print_operator_json();
    } else {
        println!("pack capabilities");
        println!("operator contract: pack --describe");
        println!("doctor: pack doctor --json");
        println!("triage: pack doctor triage --json");
        println!("robot docs: pack robot-docs guide");
    }
    ExitCode::Success.into()
}

pub fn dispatch_robot_triage() -> u8 {
    print_triage(true)
}

pub fn dispatch_robot_docs(action: Option<&RobotDocsAction>) -> u8 {
    println!("{}", robot_docs(action));
    ExitCode::Success.into()
}

fn print_health(json_mode: bool) -> u8 {
    let report = doctor_report("health");
    if json_mode {
        println!("{}", json_string(&report));
    } else {
        println!("{}", human_health(&report));
    }
    exit_for_report(&report)
}

fn print_doctor_capabilities(json_mode: bool) -> u8 {
    let report = doctor_report("capabilities");
    if json_mode {
        println!("{}", json_string(&report));
    } else {
        println!("pack doctor capabilities");
        println!("contract: pack capabilities --json");
        println!("healthy: {}", report["healthy"]);
    }
    exit_for_report(&report)
}

fn print_triage(json_mode: bool) -> u8 {
    let report = doctor_report("triage");
    if json_mode {
        println!("{}", json_string(&report));
    } else {
        println!("{}", human_health(&report));
        println!("next: pack capabilities --json");
    }
    exit_for_report(&report)
}

fn doctor_report(mode: &str) -> Value {
    let findings = health_findings();
    let healthy = !findings
        .iter()
        .any(|finding| finding["severity"].as_str() == Some("error"));
    let mut report = json!({
        "schema_version": DOCTOR_SCHEMA_VERSION,
        "mode": mode,
        "tool": "pack",
        "tool_version": env!("CARGO_PKG_VERSION"),
        "healthy": healthy,
        "findings": findings,
    });

    match mode {
        "health" => {}
        "capabilities" => report["contract"] = operator_contract(),
        "triage" => {
            let actions: &[&str] = if healthy {
                &["pack capabilities --json"]
            } else {
                &["pack doctor --json", "pack doctor capabilities --json"]
            };
            report["next_actions"] = json!(actions);
        }
        _ => unreachable!("doctor mode is fixed by the caller"),
    }
    report
}

fn health_findings() -> Vec<Value> {
    [
        check_operator_manifest(),
        check_pack_schema(),
        check_witness_path_resolution(),
        check_artifact_command_contract(),
        check_transport_contract(),
    ]
    .into_iter()
    .map(|check| {
        let name = string_field(&check, "name").unwrap_or("unknown");
        let status = string_field(&check, "status").unwrap_or("warn");
        let severity = match status {
            "error" => "error",
            "warn" => "warning",
            _ => "info",
        };
        let mut finding = json!({
            "id": format!("pack.{name}"),
            "severity": severity,
            "message": string_field(&check, "message").unwrap_or("doctor check completed"),
        });
        if status == "error" {
            finding["fix"] = json!("inspect pack doctor capabilities --json");
        }
        finding
    })
    .collect()
}

fn operator_contract() -> Value {
    serde_json::from_str(OPERATOR_JSON).expect("operator.json must be valid JSON")
}

fn check_operator_manifest() -> Value {
    match serde_json::from_str::<Value>(OPERATOR_JSON) {
        Ok(value) => {
            let schema = string_field(&value, "schema_version") == Some("operator.v1");
            let name = string_field(&value, "name") == Some("pack");
            let version = value
                .get("tool_version")
                .or_else(|| value.get("version"))
                .and_then(Value::as_str)
                == Some(env!("CARGO_PKG_VERSION"));
            let commands = ["capabilities", "robot-docs", "doctor"]
                .iter()
                .all(|expected| has_command(&value, expected));
            let ok = schema && name && version && commands;
            check(
                "operator_manifest",
                if ok { "ok" } else { "error" },
                if ok {
                    "Compiled operator manifest matches the current binary."
                } else {
                    "Compiled operator manifest does not match the current binary."
                },
                json!({
                    "schema_version": schema,
                    "name": name,
                    "tool_version": version,
                    "commands": commands
                }),
            )
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
    let has_manifest = definitions.is_some_and(|defs| defs.contains_key("manifest"));
    let has_verify_report = definitions.is_some_and(|defs| defs.contains_key("verify_report"));
    let ok = id == Some("pack.v0")
        && title == Some("pack.v0 manifest and verify schema")
        && has_manifest
        && has_verify_report;
    check(
        "pack_schema",
        if ok { "ok" } else { "error" },
        if ok {
            "Compiled schema advertises pack.v0 manifest and verify report definitions."
        } else {
            "Compiled schema is missing expected pack.v0 metadata."
        },
        json!({
            "id": id,
            "title": title,
            "has_manifest_definition": has_manifest,
            "has_verify_report_definition": has_verify_report
        }),
    )
}

fn check_witness_path_resolution() -> Value {
    let path = crate::witness::witness_ledger_path();
    check(
        "witness_path_resolution",
        "ok",
        "Resolved witness ledger path without creating directories or appending records.",
        json!({
            "path": path.display().to_string(),
            "parent": path.parent().map(Path::to_path_buf).map(|value| value.display().to_string()),
            "write_attempted": false
        }),
    )
}

fn check_artifact_command_contract() -> Value {
    check(
        "artifact_command_contract",
        "ok",
        "Doctor commands are outside seal, verify, inspect, diff, and archive paths.",
        json!({"seal": false, "verify": false, "inspect": false, "diff": false, "archive": false, "witness_append": false}),
    )
}

fn check_transport_contract() -> Value {
    check(
        "transport_contract",
        "ok",
        "Doctor commands do not read network configuration or open network connections.",
        json!({"uses_network": false, "reads_network_env": false}),
    )
}

fn check(name: &str, status: &str, message: &str, details: Value) -> Value {
    json!({"name": name, "status": status, "message": message, "details": details})
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn has_command(value: &Value, name: &str) -> bool {
    value
        .get("commands")
        .and_then(Value::as_array)
        .is_some_and(|commands| {
            commands
                .iter()
                .any(|command| string_field(command, "name") == Some(name))
        })
}

fn human_health(report: &Value) -> String {
    let healthy = report["healthy"].as_bool().unwrap_or(false);
    let findings = report["findings"].as_array().map(Vec::len).unwrap_or(0);
    format!(
        "pack doctor {}: {findings} findings",
        if healthy { "healthy" } else { "unhealthy" }
    )
}

fn robot_docs(action: Option<&RobotDocsAction>) -> String {
    match action {
        Some(RobotDocsAction::Guide) | None => {}
    }
    [
        "# pack robot-docs guide",
        "",
        "Use `pack --robot-triage` for a single read-only triage report.",
        "Use `pack capabilities --json` for the operator.v1 contract.",
        "Use `pack doctor --json` for health, `pack doctor capabilities --json` for the embedded contract, and `pack doctor triage --json` for next actions.",
        "Legacy aliases remain supported: `pack doctor health --json` and `pack doctor --robot-triage`.",
        "Use `pack seal ... --out <DIR>`; `--output` remains a compatibility alias.",
        "NO_COLOR=1 and non-TTY output are ANSI-free.",
        "Repair policy: `pack doctor --fix` is unavailable and exits 2 without stdout.",
    ]
    .join("\n")
}

fn fix_unavailable() -> u8 {
    eprintln!("pack doctor --fix is unavailable: diagnostics are read-only in this release.");
    eprintln!("Try --robot-triage: pack --robot-triage");
    eprintln!("Try capabilities --json: pack capabilities --json");
    eprintln!("Try robot-docs guide: pack robot-docs guide");
    ExitCode::Refusal.into()
}

fn exit_for_report(report: &Value) -> u8 {
    if report["healthy"].as_bool() == Some(true) {
        ExitCode::Success.into()
    } else {
        ExitCode::Invalid.into()
    }
}

fn print_operator_json() {
    println!("{}", json_string(&operator_contract()));
}

fn json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| {
        format!("{{\"schema_version\":\"{DOCTOR_SCHEMA_VERSION}\",\"mode\":\"health\",\"tool\":\"pack\",\"tool_version\":\"{}\",\"healthy\":false,\"findings\":[{{\"id\":\"pack.render\",\"severity\":\"error\",\"message\":\"serialization failed: {error}\"}}]}}", env!("CARGO_PKG_VERSION"))
    })
}
