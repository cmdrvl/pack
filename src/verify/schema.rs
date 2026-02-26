use std::fs;
use std::path::Path;

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
        "canon.v0" | "assess.v0" => Some(validate_artifact_v0),
        "verify.rules.v0" => Some(validate_rules_v0),
        "pack.v0" => Some(validate_pack_v0),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn member(path: &str, version: Option<&str>) -> Member {
        Member {
            path: path.to_string(),
            bytes_hash: "sha256:placeholder".to_string(),
            member_type: "test".to_string(),
            artifact_version: version.map(|v| v.to_string()),
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
