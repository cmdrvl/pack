/// Result of member type detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberTypeResult {
    /// Detected type string for the manifest.
    pub member_type: String,
    /// Parsed artifact version, if available.
    pub artifact_version: Option<String>,
}

/// Detect member type and artifact version from file content.
///
/// Detection rules (from plan contract):
/// - `lock.v0` → `lockfile`
/// - `rvl.v0`, `shape.v0`, `verify.v0`, `compare.v0` → `report`
/// - `canon.v0`, `assess.v0` → `artifact`
/// - `verify.rules.v0` → `rules`
/// - `pack.v0` → `pack`
/// - YAML with `schema_version` + `profile_id` → `profile`
/// - Registry artifacts (`registry.json`, registry tables) → `registry`
/// - Everything else → `other`
pub fn detect_member_type(content: &[u8], path: &str) -> MemberTypeResult {
    // Try JSON detection first.
    if let Ok(text) = std::str::from_utf8(content) {
        if let Some(result) = detect_from_json(text) {
            return result;
        }
        if let Some(result) = detect_from_yaml(text) {
            return result;
        }
    }

    // Registry heuristic by filename.
    if is_registry_path(path) {
        return MemberTypeResult {
            member_type: "registry".to_string(),
            artifact_version: None,
        };
    }

    MemberTypeResult {
        member_type: "other".to_string(),
        artifact_version: None,
    }
}

/// Attempt to detect type from JSON content by looking for a `version` field.
fn detect_from_json(text: &str) -> Option<MemberTypeResult> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let version = value.get("version")?.as_str()?;

    match version {
        "lock.v0" => Some(MemberTypeResult {
            member_type: "lockfile".to_string(),
            artifact_version: Some("lock.v0".to_string()),
        }),
        "rvl.v0" | "shape.v0" | "verify.v0" | "compare.v0" => Some(MemberTypeResult {
            member_type: "report".to_string(),
            artifact_version: Some(version.to_string()),
        }),
        "canon.v0" | "assess.v0" => Some(MemberTypeResult {
            member_type: "artifact".to_string(),
            artifact_version: Some(version.to_string()),
        }),
        "verify.rules.v0" => Some(MemberTypeResult {
            member_type: "rules".to_string(),
            artifact_version: Some("verify.rules.v0".to_string()),
        }),
        "pack.v0" => Some(MemberTypeResult {
            member_type: "pack".to_string(),
            artifact_version: Some("pack.v0".to_string()),
        }),
        _ => None,
    }
}

/// Attempt to detect YAML profile (schema_version + profile_id).
fn detect_from_yaml(text: &str) -> Option<MemberTypeResult> {
    // Simple line-based detection — avoid pulling in a YAML parser.
    let has_schema_version = text.lines().any(|l| {
        let trimmed = l.trim();
        trimmed.starts_with("schema_version:")
    });
    let has_profile_id = text.lines().any(|l| {
        let trimmed = l.trim();
        trimmed.starts_with("profile_id:")
    });

    if has_schema_version && has_profile_id {
        Some(MemberTypeResult {
            member_type: "profile".to_string(),
            artifact_version: None,
        })
    } else {
        None
    }
}

/// Check if the path suggests a registry artifact.
fn is_registry_path(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or(path);
    basename == "registry.json"
        || basename.ends_with(".registry.json")
        || path.contains("registry/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lockfile() {
        let content = br#"{"version": "lock.v0", "data": {}}"#;
        let result = detect_member_type(content, "nov.lock.json");
        assert_eq!(result.member_type, "lockfile");
        assert_eq!(result.artifact_version.as_deref(), Some("lock.v0"));
    }

    #[test]
    fn detects_rvl_report() {
        let content = br#"{"version": "rvl.v0", "outcome": "NO_REAL_CHANGE"}"#;
        let result = detect_member_type(content, "rvl.report.json");
        assert_eq!(result.member_type, "report");
        assert_eq!(result.artifact_version.as_deref(), Some("rvl.v0"));
    }

    #[test]
    fn detects_shape_report() {
        let content = br#"{"version": "shape.v0"}"#;
        let result = detect_member_type(content, "shape.report.json");
        assert_eq!(result.member_type, "report");
        assert_eq!(result.artifact_version.as_deref(), Some("shape.v0"));
    }

    #[test]
    fn detects_verify_report() {
        let content = br#"{"version": "verify.v0"}"#;
        let result = detect_member_type(content, "verify.report.json");
        assert_eq!(result.member_type, "report");
        assert_eq!(result.artifact_version.as_deref(), Some("verify.v0"));
    }

    #[test]
    fn detects_compare_report() {
        let content = br#"{"version": "compare.v0"}"#;
        let result = detect_member_type(content, "compare.report.json");
        assert_eq!(result.member_type, "report");
        assert_eq!(result.artifact_version.as_deref(), Some("compare.v0"));
    }

    #[test]
    fn detects_canon_artifact() {
        let content = br#"{"version": "canon.v0"}"#;
        let result = detect_member_type(content, "canon.json");
        assert_eq!(result.member_type, "artifact");
        assert_eq!(result.artifact_version.as_deref(), Some("canon.v0"));
    }

    #[test]
    fn detects_assess_artifact() {
        let content = br#"{"version": "assess.v0"}"#;
        let result = detect_member_type(content, "assess.json");
        assert_eq!(result.member_type, "artifact");
        assert_eq!(result.artifact_version.as_deref(), Some("assess.v0"));
    }

    #[test]
    fn detects_rules() {
        let content = br#"{"version": "verify.rules.v0", "rules": []}"#;
        let result = detect_member_type(content, "rules.json");
        assert_eq!(result.member_type, "rules");
        assert_eq!(result.artifact_version.as_deref(), Some("verify.rules.v0"));
    }

    #[test]
    fn detects_pack() {
        let content = br#"{"version": "pack.v0", "pack_id": "sha256:abc"}"#;
        let result = detect_member_type(content, "manifest.json");
        assert_eq!(result.member_type, "pack");
        assert_eq!(result.artifact_version.as_deref(), Some("pack.v0"));
    }

    #[test]
    fn detects_yaml_profile() {
        let content = b"schema_version: 1\nprofile_id: loan_tape_v2\nfields:\n  - name: loan_id";
        let result = detect_member_type(content, "profile.yaml");
        assert_eq!(result.member_type, "profile");
        assert_eq!(result.artifact_version, None);
    }

    #[test]
    fn detects_registry_by_filename() {
        let content = b"not json";
        let result = detect_member_type(content, "registry.json");
        assert_eq!(result.member_type, "registry");
    }

    #[test]
    fn detects_registry_by_path() {
        let content = b"data";
        let result = detect_member_type(content, "registry/loans.csv");
        assert_eq!(result.member_type, "registry");
    }

    #[test]
    fn unknown_json_falls_to_other() {
        let content = br#"{"version": "unknown.v99"}"#;
        let result = detect_member_type(content, "mystery.json");
        assert_eq!(result.member_type, "other");
        assert_eq!(result.artifact_version, None);
    }

    #[test]
    fn non_json_non_yaml_falls_to_other() {
        let content = b"just some text";
        let result = detect_member_type(content, "readme.txt");
        assert_eq!(result.member_type, "other");
    }

    #[test]
    fn binary_content_falls_to_other() {
        let content = &[0xFF, 0xFE, 0x00, 0x01, 0x02];
        let result = detect_member_type(content, "data.bin");
        assert_eq!(result.member_type, "other");
    }
}
