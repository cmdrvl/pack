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

impl MemberType {
    /// Detect member type from file content or name patterns
    pub fn detect(path: &str, content: &[u8]) -> Self {
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
            }
        }

        // Check for registry artifacts by filename
        if path.ends_with("registry.json") || path.contains("registry/") {
            return MemberType::Registry;
        }

        MemberType::Other
    }
}