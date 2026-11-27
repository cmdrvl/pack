use std::fs;
use std::path::Path;

use crate::seal::collect::is_safe_member_path;

use super::report::InvalidFinding;
use crate::seal::manifest::Member;

/// Result of schema validation across all members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaOutcome {
    /// All known members passed schema validation.
    Pass,
    /// At least one known member failed schema validation.
    Fail,
    /// No known schemas applied (all members are "other"/"registry" or type has no schema).
    Skipped,
}

impl SchemaOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            SchemaOutcome::Pass => "pass",
            SchemaOutcome::Fail => "fail",
            SchemaOutcome::Skipped => "skipped",
        }
    }
}

/// Run schema validation on all members that have a known artifact_version.
///
/// Reads each member file from `pack_dir`, parses it, and checks required
/// fields for the declared artifact version. Returns (outcome, findings).
pub fn validate_schemas(
    members: &[Member],
    pack_dir: &Path,
) -> (SchemaOutcome, Vec<InvalidFinding>) {
    let mut findings = Vec::new();
    let mut checked = 0u32;

    for member in members {
        // Path validation owns the unsafe-path refusal. Do not resolve an
        // unsafe member against the host filesystem during schema validation.
        if !is_safe_member_path(&member.path) {
            continue;
        }

        let version = match &member.artifact_version {
            Some(v) => v.as_str(),
            None => continue, // No artifact_version → skip
        };

        // Only validate types that have a local schema definition.
        let validator = match schema_for_version(version) {
            Some(v) => v,
            None => continue, // Known type but no schema yet → skip
        };

        checked += 1;

        let member_path = pack_dir.join(&member.path);
        let content = match fs::read(&member_path) {
            Ok(c) => c,
            Err(_) => continue, // Missing file is caught by hash checks, not schema
        };

        if let Err(reason) = validator(&content) {
            findings.push(InvalidFinding {
                code: "SCHEMA_VIOLATION".to_string(),
                path: Some(member.path.clone()),
                expected: Some(format!("valid {version} schema")),
                actual: Some(reason),
            });
        }
    }

    if checked == 0 {
        return (SchemaOutcome::Skipped, findings);
    }

    if findings.is_empty() {
        (SchemaOutcome::Pass, findings)
    } else {
        (SchemaOutcome::Fail, findings)
    }
}

type Validator = fn(&[u8]) -> Result<(), String>;

/// Return a compiled-in schema validator for a known artifact version, or None.
fn schema_for_version(version: &str) -> Option<Validator> {
    match version {
        "lock.v0" => Some(validate_lock_v0),
        "rvl.v0" | "shape.v0" | "verify.v0" | "compare.v0" => Some(validate_report_v0),
        "verify.report.v1" => Some(validate_verify_report_v1),
        "canon.v0" | "assess.v0" => Some(validate_artifact_v0),
        "verify.rules.v0" => Some(validate_rules_v0),
        "verify.constraint.v1" => Some(validate_verify_constraint_v1),
        "pack.v0" => Some(validate_pack_v0),
        _ => None,
    }
}

/// Return true when a member artifact version has a local schema validator.
pub fn has_schema_for_version(version: &str) -> bool {
    schema_for_version(version).is_some()
}

/// lock.v0: JSON object with "version" == "lock.v0"
fn validate_lock_v0(content: &[u8]) -> Result<(), String> {
    let value = parse_json(content)?;
    check_version_field(&value, "lock.v0")
}

/// Report types: JSON object with matching "version" field.
fn validate_report_v0(content: &[u8]) -> Result<(), String> {
    let value = parse_json(content)?;
    // Just require it's an object with a version field matching a known report version.
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing \"version\" field".to_string())?;
    match version {
        "rvl.v0" | "shape.v0" | "verify.v0" | "compare.v0" => Ok(()),
        other => Err(format!("unexpected version \"{other}\"")),
    }
}

/// verify.report.v1: validate the stable report envelope and rule summary.
fn validate_verify_report_v1(content: &[u8]) -> Result<(), String> {
    let value = parse_json(content)?;
    check_version_field(&value, "verify.report.v1")?;
    check_string_field(&value, "tool", "verify")?;
    check_enum_field(&value, "execution_mode", &["batch", "embedded"])?;
    check_enum_field(&value, "outcome", &["PASS", "FAIL", "REFUSAL"])?;
    check_nonempty_string_field(&value, "constraint_set_id")?;
    check_constraint_hash(&value)?;
    require_object_field(&value, "bindings")?;
    require_object_field(&value, "summary")?;
    require_object_field(&value, "policy_signals")?;
    require_array_field(&value, "results")?;
    if !value
        .get("refusal")
        .is_some_and(|refusal| refusal.is_null() || refusal.is_object())
    {
        return Err("missing or invalid \"refusal\" field".to_string());
    }

    let summary = value.get("summary").expect("checked above");
    for field in ["total_rules", "passed_rules", "failed_rules"] {
        if !summary.get(field).is_some_and(serde_json::Value::is_u64) {
            return Err(format!("missing or non-integer summary.\"{field}\" field"));
        }
    }
    require_object_field(summary, "by_severity")?;
    Ok(())
}

/// Grounding reports use the reserved constraint set and grounding rule id
/// namespace emitted by `verify ground`. A released ground refusal has no rule
/// results, but carries an object-valued refusal. This is a structural protocol
/// marker, not provenance: pack cannot prove which command produced arbitrary
/// member bytes, so it still verifies the member hash and every report field
/// here.
fn is_ground_report(value: &serde_json::Value) -> bool {
    let structural_marker = value
        .get("constraint_set_id")
        .and_then(serde_json::Value::as_str)
        == Some("grounding.v1")
        && value
            .get("execution_mode")
            .and_then(serde_json::Value::as_str)
            == Some("batch");
    if !structural_marker {
        return false;
    }

    let Some(results) = value.get("results").and_then(serde_json::Value::as_array) else {
        return false;
    };
    if results.is_empty() {
        // Released verify emits REFUSAL reports with no rule results. The
        // object-valued refusal is the stable marker that distinguishes this
        // case from a malformed or empty PASS/FAIL report.
        return value.get("outcome").and_then(serde_json::Value::as_str) == Some("REFUSAL")
            && value
                .get("refusal")
                .is_some_and(serde_json::Value::is_object);
    }

    results.iter().all(|result| {
        result
            .get("rule_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|rule_id| rule_id.starts_with("grounding."))
    })
}

fn check_constraint_hash(value: &serde_json::Value) -> Result<(), String> {
    let ground = is_ground_report(value);
    if ground {
        if value.get("constraint_hash").is_none() {
            return Ok(());
        }
        if let Some(hash) = value
            .get("constraint_hash")
            .and_then(|value| value.as_str())
        {
            if is_keyed_redaction_hash(hash) {
                return Ok(());
            }
        }
    }
    check_prefixed_string_field(value, "constraint_hash", "sha256:")
}

fn is_keyed_redaction_hash(value: &str) -> bool {
    value
        .strip_prefix("redacted:hmac-sha256:")
        .is_some_and(|hex| {
            hex.len() == 32
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
}

/// verify.constraint.v1: validate the compiled constraint envelope.
fn validate_verify_constraint_v1(content: &[u8]) -> Result<(), String> {
    let value = parse_json(content)?;
    check_version_field(&value, "verify.constraint.v1")?;
    check_nonempty_string_field(&value, "constraint_set_id")?;
    require_array_field(&value, "bindings")?;
    require_array_field(&value, "rules")?;
    Ok(())
}

/// Artifact types (canon.v0, assess.v0): JSON object with matching "version".
fn validate_artifact_v0(content: &[u8]) -> Result<(), String> {
    let value = parse_json(content)?;
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing \"version\" field".to_string())?;
    match version {
        "canon.v0" | "assess.v0" => Ok(()),
        other => Err(format!("unexpected version \"{other}\"")),
    }
}

/// verify.rules.v0: JSON object with "version" == "verify.rules.v0" and "rules" array.
fn validate_rules_v0(content: &[u8]) -> Result<(), String> {
    let value = parse_json(content)?;
    check_version_field(&value, "verify.rules.v0")?;
    if !value.get("rules").is_some_and(|r| r.is_array()) {
        return Err("missing or non-array \"rules\" field".to_string());
    }
    Ok(())
}

/// pack.v0: JSON object with "version" == "pack.v0", "pack_id", "members" array.
fn validate_pack_v0(content: &[u8]) -> Result<(), String> {
    let value = parse_json(content)?;
    check_version_field(&value, "pack.v0")?;
    if value.get("pack_id").and_then(|v| v.as_str()).is_none() {
        return Err("missing \"pack_id\" field".to_string());
    }
    if !value.get("members").is_some_and(|m| m.is_array()) {
        return Err("missing or non-array \"members\" field".to_string());
    }
    Ok(())
}

fn parse_json(content: &[u8]) -> Result<serde_json::Value, String> {
    let text =
        std::str::from_utf8(content).map_err(|_| "content is not valid UTF-8".to_string())?;
    serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))
}

fn check_version_field(value: &serde_json::Value, expected: &str) -> Result<(), String> {
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing \"version\" field".to_string())?;
    if version != expected {
        return Err(format!(
            "expected version \"{expected}\", got \"{version}\""
        ));
    }
    Ok(())
}

fn check_string_field(
    value: &serde_json::Value,
    field: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = value
        .get(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("missing string \"{field}\" field"))?;
    if actual != expected {
        return Err(format!(
            "expected \"{field}\" to be \"{expected}\", got \"{actual}\""
        ));
    }
    Ok(())
}

fn check_enum_field(
    value: &serde_json::Value,
    field: &str,
    allowed: &[&str],
) -> Result<(), String> {
    let actual = value
        .get(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("missing string \"{field}\" field"))?;
    if !allowed.contains(&actual) {
        return Err(format!("unexpected \"{field}\" value \"{actual}\""));
    }
    Ok(())
}

fn check_nonempty_string_field(value: &serde_json::Value, field: &str) -> Result<(), String> {
    let actual = value
        .get(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("missing string \"{field}\" field"))?;
    if actual.is_empty() {
        return Err(format!("\"{field}\" must not be empty"));
    }
    Ok(())
}

fn check_prefixed_string_field(
    value: &serde_json::Value,
    field: &str,
    prefix: &str,
) -> Result<(), String> {
    let actual = value
        .get(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("missing string \"{field}\" field"))?;
    if !actual.starts_with(prefix) {
        return Err(format!("\"{field}\" must start with \"{prefix}\""));
    }
    Ok(())
}

fn require_object_field(value: &serde_json::Value, field: &str) -> Result<(), String> {
    if !value.get(field).is_some_and(serde_json::Value::is_object) {
        return Err(format!("missing or non-object \"{field}\" field"));
    }
    Ok(())
}

fn require_array_field(value: &serde_json::Value, field: &str) -> Result<(), String> {
    if !value.get(field).is_some_and(serde_json::Value::is_array) {
        return Err(format!("missing or non-array \"{field}\" field"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(path: &str, version: Option<&str>) -> Member {
        Member {
            path: path.to_string(),
            bytes_hash: "sha256:placeholder".to_string(),
            member_type: "test".to_string(),
            artifact_version: version.map(|v| v.to_string()),
            profile_frozen: None,
            profile_sha256: None,
            column_registry_hash: None,
        }
    }

    #[test]
    fn skipped_when_no_known_members() {
        let members = vec![member("data.csv", None), member("readme.txt", None)];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("data.csv"), "a,b\n1,2").unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "hello").unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Skipped);
        assert!(findings.is_empty());
    }

    #[test]
    fn pass_when_valid_lock() {
        let members = vec![member("nov.lock.json", Some("lock.v0"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("nov.lock.json"),
            r#"{"version":"lock.v0","rows":10}"#,
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Pass);
        assert!(findings.is_empty());
    }

    #[test]
    fn pass_when_valid_report() {
        let members = vec![member("rvl.report.json", Some("rvl.v0"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("rvl.report.json"),
            r#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE"}"#,
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Pass);
        assert!(findings.is_empty());
    }

    #[test]
    fn pass_when_valid_rules() {
        let members = vec![member("rules.json", Some("verify.rules.v0"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("rules.json"),
            r#"{"version":"verify.rules.v0","rules":[{"field":"id","check":"not_null"}]}"#,
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Pass);
        assert!(findings.is_empty());
    }

    #[test]
    fn pass_when_valid_verify_v1_report() {
        let members = vec![member("report.json", Some("verify.report.v1"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("report.json"),
            include_str!("../../fixtures/artifacts/gt9.verify.report.v1.json"),
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Pass);
        assert!(findings.is_empty());
    }

    fn verify_report(
        constraint_set_id: &str,
        constraint_hash: Option<&str>,
        rule_ids: &[&str],
    ) -> Vec<u8> {
        let mut report = serde_json::json!({
            "tool": "verify",
            "version": "verify.report.v1",
            "execution_mode": "batch",
            "outcome": "PASS",
            "constraint_set_id": constraint_set_id,
            "bindings": {},
            "summary": {
                "total_rules": rule_ids.len(),
                "passed_rules": rule_ids.len(),
                "failed_rules": 0,
                "by_severity": {"error": 0, "warn": 0}
            },
            "policy_signals": {},
            "results": rule_ids.iter().map(|rule_id| serde_json::json!({
                "rule_id": rule_id,
                "severity": "error",
                "status": "pass",
                "violation_count": 0,
                "affected": []
            })).collect::<Vec<_>>(),
            "refusal": null
        });
        if let Some(hash) = constraint_hash {
            report["constraint_hash"] = serde_json::Value::String(hash.to_owned());
        }
        serde_json::to_vec(&report).unwrap()
    }

    #[test]
    fn ground_report_accepts_sha256_keyed_or_absent_constraint_hash() {
        let sha256 = format!("sha256:{}", "a".repeat(64));
        let keyed = format!("redacted:hmac-sha256:{}", "b".repeat(32));
        for hash in [Some(sha256.as_str()), Some(keyed.as_str()), None] {
            assert_eq!(
                validate_verify_report_v1(&verify_report(
                    "grounding.v1",
                    hash,
                    &["grounding.quote.quote"],
                )),
                Ok(())
            );
        }
    }

    #[test]
    fn released_ground_refusals_accept_keyed_or_absent_constraint_hash() {
        let refusal_fixtures = [
            include_str!("../../tests/fixtures/ground_reports/report-drift-refusal.json"),
            include_str!("../../tests/fixtures/ground_reports/report-bad-claims-refusal.json"),
            include_str!("../../tests/fixtures/ground_reports/report-empty-claims-refusal.json"),
            include_str!("../../tests/fixtures/ground_reports/report-missing-lock-refusal.json"),
        ];
        let keyed = "redacted:hmac-sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        for fixture in refusal_fixtures {
            let mut report: serde_json::Value = serde_json::from_str(fixture).unwrap();
            assert_eq!(report["outcome"], "REFUSAL");
            assert_eq!(report["constraint_set_id"], "grounding.v1");
            assert_eq!(report["execution_mode"], "batch");
            assert_eq!(report["results"], serde_json::json!([]));
            assert!(report["refusal"].is_object());

            report["constraint_hash"] = serde_json::Value::String(keyed.to_owned());
            assert_eq!(
                validate_verify_report_v1(&serde_json::to_vec(&report).unwrap()),
                Ok(()),
                "released ground refusal must accept keyed redaction"
            );

            report.as_object_mut().unwrap().remove("constraint_hash");
            assert_eq!(
                validate_verify_report_v1(&serde_json::to_vec(&report).unwrap()),
                Ok(()),
                "released ground refusal must accept an absent constraint hash"
            );
        }
    }

    #[test]
    fn released_ground_report_fixtures_validate_as_ground_reports() {
        let fixtures = [
            (
                "PASS",
                include_str!("../../tests/fixtures/ground_reports/report-pass.json"),
            ),
            (
                "FAIL",
                include_str!("../../tests/fixtures/ground_reports/report-fail.json"),
            ),
            (
                "REFUSAL",
                include_str!("../../tests/fixtures/ground_reports/report-drift-refusal.json"),
            ),
            (
                "REFUSAL",
                include_str!("../../tests/fixtures/ground_reports/report-bad-claims-refusal.json"),
            ),
            (
                "REFUSAL",
                include_str!(
                    "../../tests/fixtures/ground_reports/report-empty-claims-refusal.json"
                ),
            ),
            (
                "REFUSAL",
                include_str!(
                    "../../tests/fixtures/ground_reports/report-missing-lock-refusal.json"
                ),
            ),
        ];

        for (outcome, fixture) in fixtures {
            let report: serde_json::Value = serde_json::from_str(fixture).unwrap();
            assert_eq!(report["outcome"], outcome);
            assert_eq!(report["constraint_set_id"], "grounding.v1");
            assert_eq!(report["execution_mode"], "batch");
            assert_eq!(
                validate_verify_report_v1(fixture.as_bytes()),
                Ok(()),
                "released {outcome} fixture must validate"
            );
        }
    }

    #[test]
    fn empty_results_are_ground_only_for_refusals_with_refusal_objects() {
        let mut pass: serde_json::Value =
            serde_json::from_slice(&verify_report("grounding.v1", None, &[])).unwrap();
        assert!(validate_verify_report_v1(&serde_json::to_vec(&pass).unwrap()).is_err());

        pass["outcome"] = serde_json::Value::String("FAIL".to_owned());
        assert!(validate_verify_report_v1(&serde_json::to_vec(&pass).unwrap()).is_err());

        pass["outcome"] = serde_json::Value::String("REFUSAL".to_owned());
        pass["refusal"] = serde_json::Value::Null;
        assert!(validate_verify_report_v1(&serde_json::to_vec(&pass).unwrap()).is_err());

        pass["refusal"] = serde_json::json!({"code": "E_IO"});
        pass.as_object_mut().unwrap().remove("results");
        assert!(validate_verify_report_v1(&serde_json::to_vec(&pass).unwrap()).is_err());

        pass["results"] = serde_json::json!({});
        assert!(validate_verify_report_v1(&serde_json::to_vec(&pass).unwrap()).is_err());

        pass["results"] = serde_json::json!([]);
        pass["constraint_set_id"] = serde_json::Value::String("other.v1".to_owned());
        assert!(validate_verify_report_v1(&serde_json::to_vec(&pass).unwrap()).is_err());
    }

    #[test]
    fn non_ground_sha256_prefix_remains_accepted() {
        assert_eq!(
            validate_verify_report_v1(&verify_report("run.v1", Some("sha256:test"), &["CHECK"])),
            Ok(())
        );
    }

    #[test]
    fn malformed_or_non_ground_constraint_hashes_remain_invalid() {
        let malformed = [
            "redacted:hmac-sha256:short",
            "redacted:hmac-sha256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ];
        for hash in malformed {
            assert!(
                validate_verify_report_v1(&verify_report(
                    "grounding.v1",
                    Some(hash),
                    &["grounding.quote.quote"],
                ))
                .is_err(),
                "malformed hash unexpectedly accepted: {hash}"
            );
        }

        let released_sha256_values = ["sha256:test", "sha256:ABCDEF", "sha256:abc", "sha256:"];
        for hash in released_sha256_values {
            assert_eq!(
                validate_verify_report_v1(&verify_report(
                    "grounding.v1",
                    Some(hash),
                    &["grounding.quote.quote"]
                )),
                Ok(()),
                "ground report changed released acceptance for {hash}"
            );
            assert_eq!(
                validate_verify_report_v1(&verify_report("run.v1", Some(hash), &["CHECK"])),
                Ok(()),
                "non-ground report changed released acceptance for {hash}"
            );
        }

        for malformed in [serde_json::Value::Null, serde_json::json!(42)] {
            let mut report: serde_json::Value = serde_json::from_slice(&verify_report(
                "grounding.v1",
                None,
                &["grounding.quote.quote"],
            ))
            .unwrap();
            report["constraint_hash"] = malformed.clone();
            assert!(
                validate_verify_report_v1(&serde_json::to_vec(&report).unwrap()).is_err(),
                "non-string constraint hash unexpectedly accepted: {report}"
            );

            let mut non_ground: serde_json::Value =
                serde_json::from_slice(&verify_report("run.v1", None, &["CHECK"])).unwrap();
            non_ground["constraint_hash"] = malformed;
            assert!(
                validate_verify_report_v1(&serde_json::to_vec(&non_ground).unwrap()).is_err(),
                "non-ground non-string constraint hash unexpectedly accepted: {non_ground}"
            );
        }

        let non_ground_keyed = verify_report(
            "run.v1",
            Some("redacted:hmac-sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            &["CHECK"],
        );
        assert!(validate_verify_report_v1(&non_ground_keyed).is_err());

        let non_ground_absent = verify_report("run.v1", None, &["CHECK"]);
        assert!(validate_verify_report_v1(&non_ground_absent).is_err());

        let fake_ground = verify_report("grounding.v1", None, &["CHECK"]);
        assert!(validate_verify_report_v1(&fake_ground).is_err());
    }

    #[test]
    fn pass_when_valid_verify_constraint_v1() {
        let members = vec![member("constraint.json", Some("verify.constraint.v1"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("constraint.json"),
            include_str!("../../fixtures/artifacts/gt9.constraint.json"),
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Pass);
        assert!(findings.is_empty());
    }

    #[test]
    fn fail_when_verify_v1_report_summary_is_missing() {
        let members = vec![member("report.json", Some("verify.report.v1"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("report.json"),
            r#"{"tool":"verify","version":"verify.report.v1","execution_mode":"batch","outcome":"PASS","constraint_set_id":"test","constraint_hash":"sha256:test","bindings":{},"policy_signals":{},"results":[],"refusal":null}"#,
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Fail);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].actual.as_ref().unwrap().contains("summary"));
    }

    #[test]
    fn fail_when_lock_has_wrong_version() {
        let members = vec![member("bad.lock.json", Some("lock.v0"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("bad.lock.json"),
            r#"{"version":"lock.v99","rows":10}"#,
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Fail);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "SCHEMA_VIOLATION");
        assert_eq!(findings[0].path.as_deref(), Some("bad.lock.json"));
    }

    #[test]
    fn fail_when_rules_missing_array() {
        let members = vec![member("rules.json", Some("verify.rules.v0"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("rules.json"),
            r#"{"version":"verify.rules.v0","rules":"not_an_array"}"#,
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Fail);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].actual.as_ref().unwrap().contains("non-array"));
    }

    #[test]
    fn fail_when_not_json() {
        let members = vec![member("data.lock.json", Some("lock.v0"))];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("data.lock.json"), "NOT JSON AT ALL").unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Fail);
        assert_eq!(findings.len(), 1);
        assert!(findings[0]
            .actual
            .as_ref()
            .unwrap()
            .contains("invalid JSON"));
    }

    #[test]
    fn mixed_pass_and_skip() {
        let members = vec![
            member("data.lock.json", Some("lock.v0")),
            member("unknown.txt", None),
        ];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("data.lock.json"),
            r#"{"version":"lock.v0","rows":5}"#,
        )
        .unwrap();
        std::fs::write(tmp.path().join("unknown.txt"), "text").unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Pass);
        assert!(findings.is_empty());
    }

    #[test]
    fn mixed_pass_and_fail() {
        let members = vec![
            member("good.lock.json", Some("lock.v0")),
            member("bad.lock.json", Some("lock.v0")),
        ];
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("good.lock.json"),
            r#"{"version":"lock.v0","rows":5}"#,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("bad.lock.json"),
            r#"{"version":"lock.v99"}"#,
        )
        .unwrap();

        let (outcome, findings) = validate_schemas(&members, tmp.path());
        assert_eq!(outcome, SchemaOutcome::Fail);
        assert_eq!(findings.len(), 1);
    }
}
