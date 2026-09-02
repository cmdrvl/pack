/// Result of member type detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberTypeResult {
    /// Detected type string for the manifest.
    pub member_type: String,
    /// Parsed artifact version, if available.
    pub artifact_version: Option<String>,
    /// True only when a YAML profile declares the frozen-profile identity pair.
    pub profile_frozen: Option<bool>,
    /// Declared frozen profile hash, copied from the profile YAML.
    pub profile_sha256: Option<String>,
    /// Declared transitive column registry hash, copied from the profile YAML.
    pub column_registry_hash: Option<String>,
}

impl MemberTypeResult {
    fn new(member_type: &str, artifact_version: Option<&str>) -> Self {
        Self {
            member_type: member_type.to_string(),
            artifact_version: artifact_version.map(ToOwned::to_owned),
            profile_frozen: None,
            profile_sha256: None,
            column_registry_hash: None,
        }
    }

    fn frozen_profile(profile_sha256: String, column_registry_hash: Option<String>) -> Self {
        Self {
            member_type: "profile".to_string(),
            artifact_version: None,
            profile_frozen: Some(true),
            profile_sha256: Some(profile_sha256),
            column_registry_hash,
        }
    }
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
        if let Some(result) = detect_fingerprint_yaml(text, path) {
            return result;
        }
    }

    // Registry heuristic by filename.
    if is_registry_path(path) {
        return MemberTypeResult::new("registry", None);
    }

    MemberTypeResult::new("other", None)
}

/// Attempt to detect type from JSON content by looking for a `version` field.
fn detect_from_json(text: &str) -> Option<MemberTypeResult> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let version = value.get("version")?.as_str()?;

    match version {
        "lock.v0" => Some(MemberTypeResult::new("lockfile", Some("lock.v0"))),
        "rvl.v0" | "shape.v0" | "verify.v0" | "compare.v0" => {
            Some(MemberTypeResult::new("report", Some(version)))
        }
        "canon.v0" | "assess.v0" => Some(MemberTypeResult::new("artifact", Some(version))),
        "verify.rules.v0" => Some(MemberTypeResult::new("rules", Some("verify.rules.v0"))),
        "pack.v0" => Some(MemberTypeResult::new("pack", Some("pack.v0"))),
        _ => None,
    }
}

/// Attempt to detect YAML profile (schema_version + profile_id).
fn detect_from_yaml(text: &str) -> Option<MemberTypeResult> {
    // Simple line-based detection — avoid pulling in a YAML parser.
    let has_schema_version = yaml_scalar_value(text, "schema_version").is_some();
    let has_profile_id = yaml_scalar_value(text, "profile_id").is_some();

    if has_schema_version && has_profile_id {
        let profile_sha256 = yaml_scalar_value(text, "profile_sha256");
        let status = yaml_scalar_value(text, "status");
        if status.as_deref() == Some("frozen") {
            if let Some(profile_sha256) = profile_sha256 {
                return Some(MemberTypeResult::frozen_profile(
                    profile_sha256,
                    yaml_scalar_value(text, "column_registry_hash"),
                ));
            }
        }

        Some(MemberTypeResult::new("profile", None))
    } else {
        None
    }
}

fn yaml_scalar_value(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?.trim();
        let value = trim_matching_yaml_quotes(value);
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn trim_matching_yaml_quotes(value: &str) -> &str {
    if value.len() < 2 {
        return value;
    }

    let mut chars = value.chars();
    let first = chars.next();
    let last = value.chars().next_back();
    match (first, last) {
        (Some('"'), Some('"')) | (Some('\''), Some('\'')) => &value[1..value.len() - 1],
        _ => value,
    }
}

/// Detect fingerprint YAML definitions (`.fp.yaml` / `.fp.yml` extension or
/// YAML containing `fingerprint_id:` + `assertions:` keys).
fn detect_fingerprint_yaml(text: &str, path: &str) -> Option<MemberTypeResult> {
    let basename = path.rsplit('/').next().unwrap_or(path);

    // Extension-based detection: *.fp.yaml or *.fp.yml
    let by_extension = basename.ends_with(".fp.yaml") || basename.ends_with(".fp.yml");

    // Content-based detection: fingerprint_id + assertions keys
    let has_fingerprint_id = text
        .lines()
        .any(|l| l.trim().starts_with("fingerprint_id:"));
    let has_assertions = text.lines().any(|l| l.trim().starts_with("assertions:"));
    let by_content = has_fingerprint_id && has_assertions;

    if by_extension || by_content {
        // Try to extract fingerprint_id value for artifact_version
        let artifact_version = text
            .lines()
            .find(|l| l.trim().starts_with("fingerprint_id:"))
            .and_then(|l| l.trim().strip_prefix("fingerprint_id:"))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|v| !v.is_empty());

        Some(MemberTypeResult {
            member_type: "fingerprint".to_string(),
            artifact_version,
            profile_frozen: None,
            profile_sha256: None,
            column_registry_hash: None,
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
        assert_eq!(result.profile_frozen, None);
        assert_eq!(result.profile_sha256, None);
        assert_eq!(result.column_registry_hash, None);
    }

    #[test]
    fn detects_frozen_profile_identity_without_changing_type() {
        let content = b"schema_version: 1\nprofile_id: csv.tape.core.v0\nprofile_sha256: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nstatus: frozen\ncolumn_registry_hash: blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";
        let result = detect_member_type(content, "profile.yaml");
        assert_eq!(result.member_type, "profile");
        assert_eq!(result.artifact_version, None);
        assert_eq!(result.profile_frozen, Some(true));
        assert_eq!(
            result.profile_sha256.as_deref(),
            Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            result.column_registry_hash.as_deref(),
            Some("blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn profile_sha_without_frozen_status_is_not_frozen_identity() {
        let content = b"schema_version: 1\nprofile_id: draft\nprofile_sha256: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nstatus: draft\ncolumn_registry_hash: blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";
        let result = detect_member_type(content, "profile.yaml");
        assert_eq!(result.member_type, "profile");
        assert_eq!(result.profile_frozen, None);
        assert_eq!(result.profile_sha256, None);
        assert_eq!(result.column_registry_hash, None);
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
    fn detects_fingerprint_yaml_by_extension() {
        let content = b"fingerprint_id: cbre-appraisal.v1\nformat: pdf\nassertions:\n  - page_count: { min: 50 }";
        let result = detect_member_type(content, "cbre-appraisal.v1.fp.yaml");
        assert_eq!(result.member_type, "fingerprint");
        assert_eq!(
            result.artifact_version.as_deref(),
            Some("cbre-appraisal.v1")
        );
    }

    #[test]
    fn detects_fingerprint_yaml_by_content() {
        let content = b"fingerprint_id: csv.v0\nformat: csv\nassertions:\n  - filename_regex: { pattern: \".*\\.csv$\" }";
        let result = detect_member_type(content, "definitions/csv.yaml");
        assert_eq!(result.member_type, "fingerprint");
        assert_eq!(result.artifact_version.as_deref(), Some("csv.v0"));
    }

    #[test]
    fn detects_fingerprint_yml_extension() {
        let content = b"fingerprint_id: test.v1\nassertions:\n  - sheet_exists: Data";
        let result = detect_member_type(content, "test.fp.yml");
        assert_eq!(result.member_type, "fingerprint");
    }

    #[test]
    fn fingerprint_yaml_without_id_or_assertions_falls_to_other() {
        let content = b"format: csv\nsome_key: value";
        let result = detect_member_type(content, "not-a-fingerprint.yaml");
        assert_eq!(result.member_type, "other");
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
