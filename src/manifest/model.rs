//! Pack manifest data model

use serde::{Deserialize, Serialize};

/// Pack manifest following the pack.v0 schema
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Always "pack.v0"
    pub version: String,

    /// Self-hash computed from canonical manifest with pack_id=""
    pub pack_id: String,

    /// ISO 8601 UTC timestamp
    pub created: String,

    /// Optional annotation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// pack semver that created the pack
    pub tool_version: String,

    /// Sorted member descriptors
    pub members: Vec<Member>,

    /// Equals number of members
    pub member_count: usize,
}

/// Member descriptor in the manifest
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    /// Relative path within pack directory
    pub path: String,

    /// sha256:<hex> of member bytes
    pub bytes_hash: String,

    /// Auto-detected member type
    #[serde(rename = "type")]
    pub member_type: MemberType,

    /// Parsed artifact version when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_version: Option<String>,
}

/// Member type classification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberType {
    Lockfile,
    Report,
    Artifact,
    Rules,
    Pack,
    Profile,
    Registry,
    Other,
}

impl Manifest {
    /// Create a new manifest with the current timestamp
    pub fn new(note: Option<String>) -> Self {
        Self {
            version: "pack.v0".to_string(),
            pack_id: String::new(), // Empty initially for self-hash computation
            created: chrono::Utc::now().to_rfc3339(),
            note,
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            members: Vec::new(),
            member_count: 0,
        }
    }

    /// Add a member to the manifest
    pub fn add_member(&mut self, member: Member) {
        self.members.push(member);
        self.members.sort_by(|a, b| a.path.cmp(&b.path));
        self.member_count = self.members.len();
    }

    /// Set the pack_id (typically after computing the self-hash)
    pub fn set_pack_id(&mut self, pack_id: String) {
        self.pack_id = pack_id;
    }

    /// Get a version of this manifest with pack_id cleared for hash computation
    pub fn for_hash_computation(&self) -> Self {
        let mut manifest = self.clone();
        manifest.pack_id = String::new();
        manifest
    }
}

impl Member {
    /// Create a new member descriptor
    pub fn new(path: String, bytes_hash: String, member_type: MemberType, artifact_version: Option<String>) -> Self {
        Self {
            path,
            bytes_hash,
            member_type,
            artifact_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_type_detection_json_versions() {
        // Test lockfile detection
        let lock_content = r#"{"version": "lock.v0", "files": []}"#;
        assert_eq!(MemberType::detect("test.lock.json", lock_content.as_bytes()), MemberType::Lockfile);

        // Test report detection
        let rvl_content = r#"{"version": "rvl.v0", "outcome": "REAL_CHANGE"}"#;
        assert_eq!(MemberType::detect("test.rvl.json", rvl_content.as_bytes()), MemberType::Report);

        let shape_content = r#"{"version": "shape.v0", "columns": []}"#;
        assert_eq!(MemberType::detect("test.shape.json", shape_content.as_bytes()), MemberType::Report);

        let verify_content = r#"{"version": "verify.v0", "violations": []}"#;
        assert_eq!(MemberType::detect("test.verify.json", verify_content.as_bytes()), MemberType::Report);

        let compare_content = r#"{"version": "compare.v0", "differences": []}"#;
        assert_eq!(MemberType::detect("test.compare.json", compare_content.as_bytes()), MemberType::Report);

        // Test artifact detection
        let canon_content = r#"{"version": "canon.v0", "canonical": true}"#;
        assert_eq!(MemberType::detect("test.canon.json", canon_content.as_bytes()), MemberType::Artifact);

        let assess_content = r#"{"version": "assess.v0", "assessment": "PASS"}"#;
        assert_eq!(MemberType::detect("test.assess.json", assess_content.as_bytes()), MemberType::Artifact);

        // Test rules detection
        let rules_content = r#"{"version": "verify.rules.v0", "rules": []}"#;
        assert_eq!(MemberType::detect("test.rules.json", rules_content.as_bytes()), MemberType::Rules);

        // Test pack detection
        let pack_content = r#"{"version": "pack.v0", "members": []}"#;
        assert_eq!(MemberType::detect("test.pack.json", pack_content.as_bytes()), MemberType::Pack);
    }

    #[test]
    fn test_member_type_detection_profile_json() {
        // Test profile detection via JSON
        let profile_content = r#"{"schema_version": "1.0", "profile_id": "test-profile"}"#;
        assert_eq!(MemberType::detect("test.profile.json", profile_content.as_bytes()), MemberType::Profile);
    }

    #[test]
    fn test_member_type_detection_profile_yaml() {
        // Test profile detection via YAML
        let yaml_content = r#"
schema_version: "1.0"
profile_id: "test-profile"
description: "Test profile"
"#;
        assert_eq!(MemberType::detect("test.profile.yaml", yaml_content.as_bytes()), MemberType::Profile);

        let yaml_content_alt = r#"
schemaVersion: "1.0"
profileId: "test-profile"
description: "Test profile"
"#;
        assert_eq!(MemberType::detect("test.profile.yml", yaml_content_alt.as_bytes()), MemberType::Profile);
    }

    #[test]
    fn test_member_type_detection_registry_by_path() {
        // Test registry detection by filename
        assert_eq!(MemberType::detect("registry.json", b"{}"), MemberType::Registry);
        assert_eq!(MemberType::detect("data/registry.json", b"{}"), MemberType::Registry);

        // Test registry detection by directory
        assert_eq!(MemberType::detect("registry/metadata.json", b"{}"), MemberType::Registry);
        assert_eq!(MemberType::detect("registry/tables/users.csv", b"id,name"), MemberType::Registry);

        // Test registry detection by naming patterns
        assert_eq!(MemberType::detect("registry_snapshot.json", b"{}"), MemberType::Registry);
        assert_eq!(MemberType::detect("user_registry.csv", b"id,name"), MemberType::Registry);
    }

    #[test]
    fn test_member_type_detection_registry_by_structure() {
        // Test registry detection by JSON structure - metadata pattern
        let registry_metadata = r#"{"registry_id": "test", "entries": []}"#;
        assert_eq!(MemberType::detect("data.json", registry_metadata.as_bytes()), MemberType::Registry);

        // Test registry detection by JSON structure - data array pattern
        let registry_data = r#"{"data": [{"id": "1", "name": "test"}]}"#;
        assert_eq!(MemberType::detect("snapshot.json", registry_data.as_bytes()), MemberType::Registry);

        // Test registry detection by JSON structure - entry array pattern
        let registry_entries = r#"[{"id": "1", "name": "test", "type": "user", "created": "2024-01-01"}]"#;
        assert_eq!(MemberType::detect("export.json", registry_entries.as_bytes()), MemberType::Registry);
    }

    #[test]
    fn test_member_type_detection_other() {
        // Test unknown JSON
        let unknown_content = r#"{"version": "unknown.v1", "data": "test"}"#;
        assert_eq!(MemberType::detect("test.json", unknown_content.as_bytes()), MemberType::Other);

        // Test plain text
        let text_content = "This is plain text content";
        assert_eq!(MemberType::detect("test.txt", text_content.as_bytes()), MemberType::Other);

        // Test binary data
        let binary_content = b"\x00\x01\x02\x03";
        assert_eq!(MemberType::detect("test.bin", binary_content), MemberType::Other);

        // Test malformed JSON
        let malformed_json = r#"{"incomplete": json"#;
        assert_eq!(MemberType::detect("bad.json", malformed_json.as_bytes()), MemberType::Other);
    }

    #[test]
    fn test_member_type_detection_deterministic() {
        let content = r#"{"version": "rvl.v0", "outcome": "NO_REAL_CHANGE"}"#;

        // Same input should always produce same output
        assert_eq!(
            MemberType::detect("test.rvl.json", content.as_bytes()),
            MemberType::detect("test.rvl.json", content.as_bytes())
        );
    }

    #[test]
    fn test_member_type_detection_precedence() {
        // Registry path takes precedence over JSON content
        let rvl_in_registry = r#"{"version": "rvl.v0", "outcome": "REAL_CHANGE"}"#;
        assert_eq!(MemberType::detect("registry/report.json", rvl_in_registry.as_bytes()), MemberType::Registry);

        // Without registry path, should detect as report
        assert_eq!(MemberType::detect("report.json", rvl_in_registry.as_bytes()), MemberType::Report);
    }

    #[test]
    fn test_manifest_new() {
        let manifest = Manifest::new(Some("test note".to_string()));

        assert_eq!(manifest.version, "pack.v0");
        assert_eq!(manifest.pack_id, "");
        assert_eq!(manifest.note, Some("test note".to_string()));
        assert_eq!(manifest.tool_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest.member_count, 0);
        assert!(manifest.members.is_empty());
    }

    #[test]
    fn test_manifest_add_member() {
        let mut manifest = Manifest::new(None);

        let member1 = Member::new(
            "b.txt".to_string(),
            "sha256:def456".to_string(),
            MemberType::Other,
            None,
        );
        let member2 = Member::new(
            "a.txt".to_string(),
            "sha256:abc123".to_string(),
            MemberType::Other,
            None,
        );

        manifest.add_member(member1);
        manifest.add_member(member2);

        // Members should be sorted by path
        assert_eq!(manifest.members.len(), 2);
        assert_eq!(manifest.member_count, 2);
        assert_eq!(manifest.members[0].path, "a.txt");
        assert_eq!(manifest.members[1].path, "b.txt");
    }

    #[test]
    fn test_manifest_for_hash_computation() {
        let mut manifest = Manifest::new(Some("test".to_string()));
        manifest.set_pack_id("sha256:original".to_string());

        let hash_manifest = manifest.for_hash_computation();

        assert_eq!(hash_manifest.pack_id, "");
        assert_eq!(hash_manifest.note, manifest.note);
        assert_eq!(hash_manifest.version, manifest.version);
        assert_eq!(hash_manifest.tool_version, manifest.tool_version);
    }
}

impl MemberType {
    /// Detect member type from file content or name patterns
    pub fn detect(path: &str, content: &[u8]) -> Self {
        // First check for registry artifacts by path patterns
        if Self::is_registry_artifact(path) {
            return MemberType::Registry;
        }

        // Try to parse as JSON and look for version markers
        if let Ok(text) = std::str::from_utf8(content) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(version) = value.get("version").and_then(|v| v.as_str()) {
                    return match version {
                        "lock.v0" => MemberType::Lockfile,
                        "rvl.v0" | "shape.v0" | "verify.v0" | "compare.v0" => MemberType::Report,
                        "canon.v0" | "assess.v0" => MemberType::Artifact,
                        "verify.rules.v0" => MemberType::Rules,
                        "pack.v0" => MemberType::Pack,
                        _ => MemberType::Other,
                    };
                }

                // Check for YAML-like schema_version + profile_id pattern
                if value.get("schema_version").is_some() && value.get("profile_id").is_some() {
                    return MemberType::Profile;
                }

                // Check for registry JSON structure (even if version is not set)
                if Self::is_registry_json_structure(&value) {
                    return MemberType::Registry;
                }
            }

            // Check for YAML content with profile markers (not JSON)
            if Self::is_profile_yaml(text) {
                return MemberType::Profile;
            }
        }

        MemberType::Other
    }

    /// Check if path indicates a registry artifact
    fn is_registry_artifact(path: &str) -> bool {
        // Direct registry.json file
        if path.ends_with("registry.json") || path == "registry.json" {
            return true;
        }

        // Files within registry directories
        if path.contains("registry/") || path.starts_with("registry/") {
            return true;
        }

        // Registry table files (common patterns)
        if path.contains("registry_") || path.contains("_registry") {
            return true;
        }

        // CSV/TSV files that might be registry tables
        if (path.ends_with(".csv") || path.ends_with(".tsv")) &&
           path.to_lowercase().contains("registry") {
            return true;
        }

        false
    }

    /// Check if JSON structure looks like registry data
    fn is_registry_json_structure(value: &serde_json::Value) -> bool {
        // Look for common registry structure patterns
        if let Some(obj) = value.as_object() {
            // Registry metadata patterns
            if obj.contains_key("registry_id") || obj.contains_key("registry_type") ||
               obj.contains_key("entries") || obj.contains_key("registry_entries") {
                return true;
            }

            // Array of registry entries
            if obj.contains_key("data") && obj.get("data").and_then(|d| d.as_array()).is_some() {
                return true;
            }
        }

        // Array of registry objects
        if let Some(arr) = value.as_array() {
            if !arr.is_empty() {
                if let Some(first) = arr.first().and_then(|v| v.as_object()) {
                    // Check if first object has registry-like fields
                    let registry_fields = ["id", "name", "type", "created", "updated"];
                    let field_count = registry_fields.iter()
                        .filter(|&field| first.contains_key(*field))
                        .count();
                    if field_count >= 3 {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if text content is YAML with profile markers
    fn is_profile_yaml(text: &str) -> bool {
        // Look for YAML-style profile markers
        let lines: Vec<&str> = text.lines().collect();
        let mut has_schema_version = false;
        let mut has_profile_id = false;

        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with("schema_version:") || trimmed.starts_with("schemaVersion:") {
                has_schema_version = true;
            }
            if trimmed.starts_with("profile_id:") || trimmed.starts_with("profileId:") {
                has_profile_id = true;
            }

            if has_schema_version && has_profile_id {
                return true;
            }
        }

        false
    }
}