use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Manifest schema version.
pub const MANIFEST_VERSION: &str = "pack.v0";

/// A member descriptor within the pack manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Member {
    pub path: String,
    pub bytes_hash: String,
    #[serde(rename = "type")]
    pub member_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_version: Option<String>,
}

/// The pack.v0 manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    pub pack_id: String,
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub tool_version: String,
    pub members: Vec<Member>,
    pub member_count: usize,
}

impl Manifest {
    /// Create a new manifest with `pack_id` set to empty (placeholder for self-hash).
    pub fn new(
        created: String,
        note: Option<String>,
        tool_version: String,
        members: Vec<Member>,
    ) -> Self {
        let member_count = members.len();
        Self {
            version: MANIFEST_VERSION.to_string(),
            pack_id: String::new(),
            created,
            note,
            tool_version,
            members,
            member_count,
        }
    }

    /// Compute and set the deterministic `pack_id` via the self-hash contract:
    ///
    /// 1. Serialize manifest with `pack_id: ""`
    /// 2. Canonical JSON (serde_json with sorted keys via `to_string`)
    /// 3. SHA256 over canonical bytes
    /// 4. Set `pack_id` to `sha256:<hex>`
    pub fn finalize(&mut self) {
        self.pack_id = String::new();
        let canonical = canonical_json(self);
        let hash = sha256_hex(canonical.as_bytes());
        self.pack_id = format!("sha256:{hash}");
    }

    /// Recompute pack_id without mutating, for verification.
    pub fn recompute_pack_id(&self) -> String {
        let mut copy = self.clone();
        copy.pack_id = String::new();
        let canonical = canonical_json(&copy);
        let hash = sha256_hex(canonical.as_bytes());
        format!("sha256:{hash}")
    }

    /// Serialize the finalized manifest to deterministic JSON bytes.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        canonical_json(self).into_bytes()
    }
}

/// Produce canonical JSON: deterministic key ordering via serde_json::Value
/// round-trip, then serialize with sorted maps.
fn canonical_json(manifest: &Manifest) -> String {
    // serde_json::to_value + to_string produces deterministic output because
    // serde_json preserves insertion order from the struct derive order.
    // For true canonical form we round-trip through Value to ensure stability.
    let value = serde_json::to_value(manifest).expect("manifest serialization cannot fail");
    sorted_json(&value)
}

/// Recursively serialize a serde_json::Value with sorted object keys.
fn sorted_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let entries: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap(),
                        sorted_json(&map[*k])
                    )
                })
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        serde_json::Value::Array(arr) => {
            let entries: Vec<String> = arr.iter().map(sorted_json).collect();
            format!("[{}]", entries.join(","))
        }
        _ => serde_json::to_string(value).unwrap(),
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_members() -> Vec<Member> {
        vec![
            Member {
                path: "a.json".to_string(),
                bytes_hash: "sha256:aaaa".to_string(),
                member_type: "report".to_string(),
                artifact_version: Some("rvl.v0".to_string()),
            },
            Member {
                path: "b.lock.json".to_string(),
                bytes_hash: "sha256:bbbb".to_string(),
                member_type: "lockfile".to_string(),
                artifact_version: Some("lock.v0".to_string()),
            },
        ]
    }

    #[test]
    fn new_manifest_has_empty_pack_id() {
        let m = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        assert_eq!(m.pack_id, "");
        assert_eq!(m.member_count, 2);
        assert_eq!(m.version, "pack.v0");
    }

    #[test]
    fn finalize_sets_pack_id() {
        let mut m = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        m.finalize();
        assert!(m.pack_id.starts_with("sha256:"));
        assert_eq!(m.pack_id.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn finalize_is_deterministic() {
        let mut m1 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        let mut m2 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        m1.finalize();
        m2.finalize();
        assert_eq!(m1.pack_id, m2.pack_id);
    }

    #[test]
    fn recompute_matches_finalized() {
        let mut m = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        m.finalize();
        let recomputed = m.recompute_pack_id();
        assert_eq!(m.pack_id, recomputed);
    }

    #[test]
    fn pack_id_changes_with_note() {
        let mut m1 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        let mut m2 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            Some("hello".to_string()),
            "0.1.0".to_string(),
            sample_members(),
        );
        m1.finalize();
        m2.finalize();
        assert_ne!(m1.pack_id, m2.pack_id);
    }

    #[test]
    fn pack_id_changes_with_created() {
        let mut m1 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        let mut m2 = Manifest::new(
            "2026-01-16T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        m1.finalize();
        m2.finalize();
        assert_ne!(m1.pack_id, m2.pack_id);
    }

    #[test]
    fn pack_id_changes_with_tool_version() {
        let mut m1 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        let mut m2 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.2.0".to_string(),
            sample_members(),
        );
        m1.finalize();
        m2.finalize();
        assert_ne!(m1.pack_id, m2.pack_id);
    }

    #[test]
    fn pack_id_changes_with_member_content() {
        let mut m1 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        let mut modified = sample_members();
        modified[0].bytes_hash = "sha256:xxxx".to_string();
        let mut m2 = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            modified,
        );
        m1.finalize();
        m2.finalize();
        assert_ne!(m1.pack_id, m2.pack_id);
    }

    #[test]
    fn canonical_json_has_sorted_keys() {
        let m = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        let json = canonical_json(&m);
        // Keys should appear in alphabetical order
        let created_pos = json.find("\"created\"").unwrap();
        let member_count_pos = json.find("\"member_count\"").unwrap();
        let members_pos = json.find("\"members\"").unwrap();
        let pack_id_pos = json.find("\"pack_id\"").unwrap();
        let tool_version_pos = json.find("\"tool_version\"").unwrap();
        let version_pos = json.find("\"version\"").unwrap();

        assert!(created_pos < member_count_pos);
        assert!(member_count_pos < members_pos);
        assert!(members_pos < pack_id_pos);
        assert!(pack_id_pos < tool_version_pos);
        assert!(tool_version_pos < version_pos);
    }

    #[test]
    fn to_canonical_bytes_is_stable() {
        let mut m = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            "0.1.0".to_string(),
            sample_members(),
        );
        m.finalize();
        let b1 = m.to_canonical_bytes();
        let b2 = m.to_canonical_bytes();
        assert_eq!(b1, b2);
    }
}
