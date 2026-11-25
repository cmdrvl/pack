//! Deterministic, redacted audit dossiers for verified evidence packs.
//!
//! Rendering is deliberately a two-phase operation: verify the closed pack and
//! every declared member first, then read only allowlisted metadata fields from
//! the verified bytes. No arbitrary member content is copied into a dossier.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::cli::RenderFormat;
use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::manifest::{Manifest, Member};
use crate::verify::execute_verify;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderResult {
    pub pack_id: String,
    pub bytes_written: usize,
}

#[derive(Debug, Clone)]
struct Dossier {
    manifest: Manifest,
    members: Vec<MemberSummary>,
    assess: Vec<AssessSummary>,
    rvl: Vec<RvlSummary>,
    witnesses: Vec<WitnessSummary>,
    lock_inputs: Vec<LockSummary>,
    airlock: Vec<AirlockSummary>,
}

#[derive(Debug, Clone)]
struct MemberSummary {
    path: String,
    member_type: String,
    artifact_version: Option<String>,
    bytes_hash: String,
}

#[derive(Debug, Clone)]
struct AssessSummary {
    path: String,
    verdict: Option<String>,
    rules: Vec<String>,
}

#[derive(Debug, Clone)]
struct RvlSummary {
    path: String,
    outcome: Option<String>,
    contributors: Vec<NumericContributor>,
    field_change_count: usize,
    rows_old: Option<u64>,
    rows_new: Option<u64>,
    rows_aligned: Option<u64>,
}

#[derive(Debug, Clone)]
struct NumericContributor {
    rank: usize,
    contribution: Option<f64>,
    share: Option<f64>,
}

#[derive(Debug, Clone)]
struct WitnessSummary {
    path: String,
    kind: String,
    result: Option<String>,
    passed: Option<u64>,
    failed: Option<u64>,
}

#[derive(Debug, Clone)]
struct LockSummary {
    path: String,
    bytes_hash: String,
    lock_hash: Option<String>,
    input_count: Option<usize>,
}

#[derive(Debug, Clone)]
struct AirlockSummary {
    path: String,
    achieved_claim: Option<String>,
    local_reads_clean: Option<bool>,
    bytes_crossed: Option<String>,
    not_crossed: Option<String>,
    signer: Option<String>,
}

/// Verify a pack and write a deterministic HTML or Markdown dossier.
pub fn execute_render(
    pack_dir: &Path,
    format: RenderFormat,
    output: &Path,
) -> Result<RenderResult, Box<RefusalEnvelope>> {
    let (verify_output, verify_code) = execute_verify(pack_dir, true);
    if verify_code != 0 {
        let report = serde_json::from_str::<Value>(&verify_output)
            .unwrap_or_else(|_| Value::String(verify_output.clone()));
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some("Cannot render an unverified pack".to_string()),
            Some(serde_json::json!({
                "verify_exit_code": verify_code,
                "verify_report": report,
            })),
        )));
    }

    let manifest_path = pack_dir.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
        io_refusal(
            "Cannot read verified manifest.json",
            serde_json::json!({"path": "manifest.json", "error": error.to_string()}),
        )
    })?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        io_refusal(
            "Cannot parse verified manifest.json",
            serde_json::json!({"path": "manifest.json", "error": error.to_string()}),
        )
    })?;

    let dossier = build_dossier(pack_dir, manifest)?;
    let bytes = match format {
        RenderFormat::Html => render_html(&dossier).into_bytes(),
        RenderFormat::Md => render_markdown(&dossier).into_bytes(),
    };
    fs::write(output, &bytes).map_err(|error| {
        io_refusal(
            "Cannot write rendered dossier",
            serde_json::json!({
                "path": output.display().to_string(),
                "error": error.to_string()
            }),
        )
    })?;

    Ok(RenderResult {
        pack_id: dossier.manifest.pack_id,
        bytes_written: bytes.len(),
    })
}

fn build_dossier(pack_dir: &Path, manifest: Manifest) -> Result<Dossier, Box<RefusalEnvelope>> {
    let mut members = Vec::with_capacity(manifest.members.len());
    let mut assess = Vec::new();
    let mut rvl = Vec::new();
    let mut witnesses = Vec::new();
    let mut lock_inputs = Vec::new();
    let mut airlock = Vec::new();

    let mut declared = manifest.members.clone();
    declared.sort_by(|left, right| left.path.cmp(&right.path));

    for member in declared {
        members.push(MemberSummary {
            path: member.path.clone(),
            member_type: member.member_type.clone(),
            artifact_version: member.artifact_version.clone(),
            bytes_hash: member.bytes_hash.clone(),
        });

        let path = pack_dir.join(&member.path);
        let bytes = fs::read(&path).map_err(|error| {
            io_refusal(
                "Cannot read verified pack member",
                serde_json::json!({"path": member.path, "error": error.to_string()}),
            )
        })?;
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };

        if value.get("version").and_then(Value::as_str) == Some("assess.v0") {
            assess.push(assess_summary(&member.path, &value));
        }
        if value.get("version").and_then(Value::as_str) == Some("rvl.v0") {
            rvl.push(rvl_summary(&member.path, &value));
        }
        if is_witness_artifact(&value, &member) {
            witnesses.push(witness_summary(&member.path, &member, &value));
        }
        if value.get("version").and_then(Value::as_str) == Some("lock.v0") {
            lock_inputs.push(lock_summary(&member, &value));
        }
        let has_boundary_fields = value.get("boundary_mode").is_some()
            || value.get("LOCAL_READS_CLEAN").is_some()
            || value.get("local_reads_clean").is_some();
        if value.get("manifest_version").and_then(Value::as_str) == Some("airlock.v0")
            || (value.get("achieved_claim").is_some() && has_boundary_fields)
        {
            airlock.push(airlock_summary(&member.path, &value));
        }
    }

    Ok(Dossier {
        manifest,
        members,
        assess,
        rvl,
        witnesses,
        lock_inputs,
        airlock,
    })
}

fn is_witness_artifact(value: &Value, member: &Member) -> bool {
    let version = value.get("version").and_then(Value::as_str).unwrap_or("");
    let name = member.path.to_ascii_lowercase();
    version == "verify.v0"
        || version == "benchmark.v0"
        || name.contains("verify")
        || name.contains("benchmark")
}

fn assess_summary(path: &str, value: &Value) -> AssessSummary {
    let mut rules = Vec::new();
    push_string(&mut rules, value.get("matched_rule"));
    if let Some(items) = value.get("risk_factors").and_then(Value::as_array) {
        for item in items {
            push_string(&mut rules, item.get("code"));
        }
    }
    rules.sort();
    rules.dedup();
    AssessSummary {
        path: path.to_string(),
        verdict: first_string(value, &["decision_band", "verdict", "outcome"]),
        rules,
    }
}

fn rvl_summary(path: &str, value: &Value) -> RvlSummary {
    let counts = value.get("counts").and_then(Value::as_object);
    let contributors = value
        .get("contributors")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(10)
                .enumerate()
                .map(|(index, item)| NumericContributor {
                    rank: index + 1,
                    contribution: item.get("contribution").and_then(Value::as_f64),
                    share: item.get("share").and_then(Value::as_f64),
                })
                .collect()
        })
        .unwrap_or_default();

    RvlSummary {
        path: path.to_string(),
        outcome: first_string(value, &["outcome", "status"]),
        contributors,
        field_change_count: value
            .get("field_changes")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        rows_old: counts
            .and_then(|map| map.get("rows_old"))
            .and_then(Value::as_u64),
        rows_new: counts
            .and_then(|map| map.get("rows_new"))
            .and_then(Value::as_u64),
        rows_aligned: counts
            .and_then(|map| map.get("rows_aligned"))
            .and_then(Value::as_u64),
    }
}

fn witness_summary(path: &str, member: &Member, value: &Value) -> WitnessSummary {
    let kind = if member
        .artifact_version
        .as_deref()
        .is_some_and(|version| version == "benchmark.v0")
        || path.to_ascii_lowercase().contains("benchmark")
    {
        "benchmark"
    } else {
        "verify"
    };
    WitnessSummary {
        path: path.to_string(),
        kind: kind.to_string(),
        result: first_string(value, &["outcome", "status", "verdict"]),
        passed: first_u64(value, &["passed", "pass_count"]),
        failed: first_u64(value, &["failed", "fail_count"]),
    }
}

fn lock_summary(member: &Member, value: &Value) -> LockSummary {
    let input_count = ["inputs", "files", "artifacts"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array).map(Vec::len));
    LockSummary {
        path: member.path.clone(),
        bytes_hash: member.bytes_hash.clone(),
        lock_hash: first_string(value, &["lock_hash", "hash"]),
        input_count,
    }
}

fn airlock_summary(path: &str, value: &Value) -> AirlockSummary {
    AirlockSummary {
        path: path.to_string(),
        achieved_claim: first_string(value, &["achieved_claim", "claim"]),
        local_reads_clean: value
            .get("LOCAL_READS_CLEAN")
            .or_else(|| value.get("local_reads_clean"))
            .and_then(Value::as_bool),
        bytes_crossed: boundary_value(value.get("bytes_crossed")),
        not_crossed: boundary_value(
            value
                .get("not_crossed")
                .or_else(|| value.get("bytes_not_crossed")),
        ),
        signer: first_string(value, &["signer", "signed_by"]),
    }
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn first_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn push_string(values: &mut Vec<String>, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str) {
        values.push(value.to_string());
    }
}

fn boundary_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Array(values) => {
            let values = values
                .iter()
                .filter_map(|value| match value {
                    Value::String(value) => Some(value.clone()),
                    Value::Number(value) => Some(value.to_string()),
                    Value::Bool(value) => Some(value.to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            Some(values.join(", "))
        }
        Value::Null | Value::Object(_) => None,
    }
}

fn render_markdown(dossier: &Dossier) -> String {
    let mut output = String::new();
    writeln!(output, "# Pack audit dossier\n").unwrap();
    writeln!(output, "- Verification badge: **VERIFIED**").unwrap();
    writeln!(output, "- Pack seal hash: `{}`", dossier.manifest.pack_id).unwrap();
    writeln!(output, "- Created: `{}`", dossier.manifest.created).unwrap();
    writeln!(
        output,
        "- Tool version: `{}`",
        dossier.manifest.tool_version
    )
    .unwrap();
    writeln!(
        output,
        "- Member count: `{}`\n",
        dossier.manifest.member_count
    )
    .unwrap();
    if let Some(note) = &dossier.manifest.note {
        writeln!(output, "- Note: {}\n", markdown_text(note)).unwrap();
    }
    if let Some(outcome) = &dossier.manifest.primary_outcome_tag {
        writeln!(output, "- Primary outcome: `{}`\n", markdown_text(outcome)).unwrap();
    }

    output.push_str("## Member inventory\n\n");
    output.push_str("| Path | Type | Artifact | Bytes hash |\n|---|---|---|---|\n");
    for member in &dossier.members {
        writeln!(
            output,
            "| `{}` | `{}` | `{}` | `{}` |",
            markdown_text(&member.path),
            markdown_text(&member.member_type),
            markdown_text(member.artifact_version.as_deref().unwrap_or("-")),
            markdown_text(&member.bytes_hash)
        )
        .unwrap();
    }
    output.push('\n');

    render_assess_markdown(&mut output, &dossier.assess);
    render_rvl_markdown(&mut output, &dossier.rvl);
    render_witness_markdown(&mut output, &dossier.witnesses);
    render_lock_markdown(&mut output, &dossier.lock_inputs);
    render_airlock_markdown(&mut output, &dossier.airlock);
    output
}

fn render_assess_markdown(output: &mut String, assessments: &[AssessSummary]) {
    output.push_str("## Assess decisions\n\n");
    if assessments.is_empty() {
        output.push_str("No assess artifact present.\n\n");
        return;
    }
    for assessment in assessments {
        writeln!(
            output,
            "- `{}` verdict: `{}`",
            assessment.path,
            assessment.verdict.as_deref().unwrap_or("UNKNOWN")
        )
        .unwrap();
        writeln!(
            output,
            "  triggered policy rules: `{}`",
            if assessment.rules.is_empty() {
                "none".to_string()
            } else {
                assessment.rules.join(", ")
            }
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_rvl_markdown(output: &mut String, reports: &[RvlSummary]) {
    output.push_str("## RVL comparison\n\n");
    if reports.is_empty() {
        output.push_str("No RVL artifact present.\n\n");
        return;
    }
    for report in reports {
        writeln!(
            output,
            "- `{}` outcome: `{}`",
            report.path,
            report.outcome.as_deref().unwrap_or("UNKNOWN")
        )
        .unwrap();
        output.push_str("- Top numeric contributors (redacted):\n");
        if report.contributors.is_empty() {
            output.push_str("  - none\n");
        } else {
            for contributor in &report.contributors {
                writeln!(
                    output,
                    "  - rank {}: contribution `{}`, share `{}`",
                    contributor.rank,
                    number_or_unknown(contributor.contribution),
                    number_or_unknown(contributor.share)
                )
                .unwrap();
            }
        }
        writeln!(
            output,
            "- Non-numeric transitions/date shifts: `{}` field changes",
            report.field_change_count
        )
        .unwrap();
        writeln!(
            output,
            "- Row churn: `{}` old → `{}` new (`{}` aligned)",
            number_or_unknown(report.rows_old),
            number_or_unknown(report.rows_new),
            number_or_unknown(report.rows_aligned)
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_witness_markdown(output: &mut String, witnesses: &[WitnessSummary]) {
    output.push_str("## Verify and benchmark witnesses\n\n");
    if witnesses.is_empty() {
        output.push_str("No verify or benchmark witness present.\n\n");
        return;
    }
    for witness in witnesses {
        writeln!(
            output,
            "- `{}` `{}` result: `{}`; passed `{}`; failed `{}`",
            witness.path,
            witness.kind,
            witness.result.as_deref().unwrap_or("UNKNOWN"),
            number_or_unknown(witness.passed),
            number_or_unknown(witness.failed)
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_lock_markdown(output: &mut String, locks: &[LockSummary]) {
    output.push_str("## Lock input provenance\n\n");
    if locks.is_empty() {
        output.push_str("No lock.v0 input present.\n\n");
        return;
    }
    for lock in locks {
        writeln!(
            output,
            "- `{}`: sealed member hash `{}`; lock hash `{}`; inputs `{}`",
            lock.path,
            lock.bytes_hash,
            lock.lock_hash.as_deref().unwrap_or("not declared"),
            number_or_unknown(lock.input_count.map(|count| count as u64))
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_airlock_markdown(output: &mut String, airlocks: &[AirlockSummary]) {
    output.push_str("## Airlock boundary\n\n");
    if airlocks.is_empty() {
        output.push_str("No airlock boundary declaration present.\n\n");
        return;
    }
    for airlock in airlocks {
        writeln!(
            output,
            "- `{}` achieved claim: `{}`",
            airlock.path,
            airlock.achieved_claim.as_deref().unwrap_or("UNKNOWN")
        )
        .unwrap();
        writeln!(
            output,
            "- LOCAL_READS_CLEAN: `{}`",
            airlock
                .local_reads_clean
                .map_or_else(|| "not declared".to_string(), |value| value.to_string())
        )
        .unwrap();
        writeln!(
            output,
            "- bytes_crossed: `{}`; not_crossed: `{}`",
            airlock.bytes_crossed.as_deref().unwrap_or("not declared"),
            airlock.not_crossed.as_deref().unwrap_or("not declared")
        )
        .unwrap();
        writeln!(
            output,
            "- signer: `{}`",
            airlock.signer.as_deref().unwrap_or("not declared")
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_html(dossier: &Dossier) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><title>Pack audit dossier</title><style>body{font-family:system-ui,sans-serif;line-height:1.45;margin:2rem;max-width:72rem}table{border-collapse:collapse;width:100%}th,td{border:1px solid #bbb;padding:.4rem;text-align:left}code{font-family:ui-monospace,monospace}h1{margin-top:0}.badge{color:#176b2c;font-weight:700}</style></head><body>");
    output
        .push_str("<h1>Pack audit dossier</h1><p class=\"badge\">Verification badge: VERIFIED</p>");
    write!(
        output,
        "<dl><dt>Pack seal hash</dt><dd><code>{}</code></dd><dt>Created</dt><dd><code>{}</code></dd><dt>Tool version</dt><dd><code>{}</code></dd><dt>Member count</dt><dd><code>{}</code></dd>",
        html_text(&dossier.manifest.pack_id),
        html_text(&dossier.manifest.created),
        html_text(&dossier.manifest.tool_version),
        dossier.manifest.member_count
    )
    .unwrap();
    if let Some(note) = &dossier.manifest.note {
        write!(output, "<dt>Note</dt><dd>{}</dd>", html_text(note)).unwrap();
    }
    if let Some(outcome) = &dossier.manifest.primary_outcome_tag {
        write!(
            output,
            "<dt>Primary outcome</dt><dd><code>{}</code></dd>",
            html_text(outcome)
        )
        .unwrap();
    }
    output.push_str("</dl><h2>Member inventory</h2><table><thead><tr><th>Path</th><th>Type</th><th>Artifact</th><th>Bytes hash</th></tr></thead><tbody>");
    for member in &dossier.members {
        write!(
            output,
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td></tr>",
            html_text(&member.path),
            html_text(&member.member_type),
            html_text(member.artifact_version.as_deref().unwrap_or("-")),
            html_text(&member.bytes_hash)
        )
        .unwrap();
    }
    output.push_str("</tbody></table>");
    render_assess_html(&mut output, &dossier.assess);
    render_rvl_html(&mut output, &dossier.rvl);
    render_witness_html(&mut output, &dossier.witnesses);
    render_lock_html(&mut output, &dossier.lock_inputs);
    render_airlock_html(&mut output, &dossier.airlock);
    output.push_str("</body></html>\n");
    output
}

fn render_assess_html(output: &mut String, assessments: &[AssessSummary]) {
    output.push_str("<h2>Assess decisions</h2>");
    if assessments.is_empty() {
        output.push_str("<p>No assess artifact present.</p>");
        return;
    }
    output.push_str("<ul>");
    for assessment in assessments {
        let rules = if assessment.rules.is_empty() {
            "none".to_string()
        } else {
            assessment.rules.join(", ")
        };
        write!(
            output,
            "<li><code>{}</code> verdict: <code>{}</code>; triggered policy rules: <code>{}</code></li>",
            html_text(&assessment.path),
            html_text(assessment.verdict.as_deref().unwrap_or("UNKNOWN")),
            html_text(&rules)
        )
        .unwrap();
    }
    output.push_str("</ul>");
}

fn render_rvl_html(output: &mut String, reports: &[RvlSummary]) {
    output.push_str("<h2>RVL comparison</h2>");
    if reports.is_empty() {
        output.push_str("<p>No RVL artifact present.</p>");
        return;
    }
    for report in reports {
        write!(
            output,
            "<h3><code>{}</code> — outcome <code>{}</code></h3><p>Non-numeric transitions/date shifts: <code>{}</code> field changes. Row churn: <code>{}</code> old → <code>{}</code> new (<code>{}</code> aligned).</p><h4>Top numeric contributors (redacted)</h4><ol>",
            html_text(&report.path),
            html_text(report.outcome.as_deref().unwrap_or("UNKNOWN")),
            report.field_change_count,
            number_or_unknown(report.rows_old),
            number_or_unknown(report.rows_new),
            number_or_unknown(report.rows_aligned)
        )
        .unwrap();
        if report.contributors.is_empty() {
            output.push_str("<li>none</li>");
        } else {
            for contributor in &report.contributors {
                write!(
                    output,
                    "<li>rank {}: contribution <code>{}</code>, share <code>{}</code></li>",
                    contributor.rank,
                    number_or_unknown(contributor.contribution),
                    number_or_unknown(contributor.share)
                )
                .unwrap();
            }
        }
        output.push_str("</ol>");
    }
}

fn render_witness_html(output: &mut String, witnesses: &[WitnessSummary]) {
    output.push_str("<h2>Verify and benchmark witnesses</h2>");
    if witnesses.is_empty() {
        output.push_str("<p>No verify or benchmark witness present.</p>");
        return;
    }
    output.push_str("<ul>");
    for witness in witnesses {
        write!(
            output,
            "<li><code>{}</code> <code>{}</code> result: <code>{}</code>; passed <code>{}</code>; failed <code>{}</code></li>",
            html_text(&witness.path),
            html_text(&witness.kind),
            html_text(witness.result.as_deref().unwrap_or("UNKNOWN")),
            number_or_unknown(witness.passed),
            number_or_unknown(witness.failed)
        )
        .unwrap();
    }
    output.push_str("</ul>");
}

fn render_lock_html(output: &mut String, locks: &[LockSummary]) {
    output.push_str("<h2>Lock input provenance</h2>");
    if locks.is_empty() {
        output.push_str("<p>No lock.v0 input present.</p>");
        return;
    }
    output.push_str("<ul>");
    for lock in locks {
        write!(
            output,
            "<li><code>{}</code>: sealed member hash <code>{}</code>; lock hash <code>{}</code>; inputs <code>{}</code></li>",
            html_text(&lock.path),
            html_text(&lock.bytes_hash),
            html_text(lock.lock_hash.as_deref().unwrap_or("not declared")),
            number_or_unknown(lock.input_count.map(|count| count as u64))
        )
        .unwrap();
    }
    output.push_str("</ul>");
}

fn render_airlock_html(output: &mut String, airlocks: &[AirlockSummary]) {
    output.push_str("<h2>Airlock boundary</h2>");
    if airlocks.is_empty() {
        output.push_str("<p>No airlock boundary declaration present.</p>");
        return;
    }
    for airlock in airlocks {
        let clean = airlock
            .local_reads_clean
            .map_or_else(|| "not declared".to_string(), |value| value.to_string());
        write!(
            output,
            "<ul><li><code>{}</code> achieved claim: <code>{}</code></li><li>LOCAL_READS_CLEAN: <code>{}</code></li><li>bytes_crossed: <code>{}</code>; not_crossed: <code>{}</code></li><li>signer: <code>{}</code></li></ul>",
            html_text(&airlock.path),
            html_text(airlock.achieved_claim.as_deref().unwrap_or("UNKNOWN")),
            html_text(&clean),
            html_text(airlock.bytes_crossed.as_deref().unwrap_or("not declared")),
            html_text(airlock.not_crossed.as_deref().unwrap_or("not declared")),
            html_text(airlock.signer.as_deref().unwrap_or("not declared"))
        )
        .unwrap();
    }
}

fn number_or_unknown<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn markdown_text(value: &str) -> String {
    value.replace(['|', '\n', '\r'], " ")
}

fn html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn io_refusal(message: &str, detail: Value) -> Box<RefusalEnvelope> {
    Box::new(RefusalEnvelope::new(
        RefusalCode::Io,
        Some(message.to_string()),
        Some(detail),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escaping_keeps_dossier_markup_closed() {
        let escaped = html_text("<raw & value>");
        assert_eq!(escaped, "&lt;raw &amp; value&gt;");
    }

    #[test]
    fn boundary_values_do_not_serialize_objects() {
        assert_eq!(
            boundary_value(Some(&serde_json::json!({"raw": "secret"}))),
            None
        );
        assert_eq!(
            boundary_value(Some(&serde_json::json!(["a", 2]))),
            Some("a, 2".to_string())
        );
    }
}
