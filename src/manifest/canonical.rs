//! Canonical JSON serialization for deterministic pack_id computation

use crate::manifest::Manifest;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Canonical serializer for deterministic JSON output
pub struct CanonicalSerializer;

impl CanonicalSerializer {
    /// Serialize a manifest to canonical JSON bytes
    pub fn serialize(manifest: &Manifest) -> anyhow::Result<Vec<u8>> {
        // First serialize to a serde_json::Value
        let json_value = serde_json::to_value(manifest)?;

        // Then canonicalize the structure
        let canonical_value = Self::canonicalize_value(json_value);

        // Finally serialize to compact bytes with deterministic ordering
        let canonical_json = serde_json::to_vec(&canonical_value)?;

        Ok(canonical_json)
    }

    /// Canonicalize a JSON value recursively
    fn canonicalize_value(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                // Convert to BTreeMap for deterministic key ordering
                let mut btree_map = BTreeMap::new();
                for (key, val) in map {
                    btree_map.insert(key, Self::canonicalize_value(val));
                }

                // Convert back to serde_json::Map to preserve ordering
                let mut ordered_map = Map::new();
                for (key, val) in btree_map {
                    ordered_map.insert(key, val);
                }

                Value::Object(ordered_map)
            }
            Value::Array(arr) => {
                // Arrays maintain their order but canonicalize contents
                Value::Array(arr.into_iter().map(Self::canonicalize_value).collect())
            }
            // Other types (null, bool, number, string) are already canonical
            other => other,
        }
    }
}

/// Convenience function to serialize a manifest to canonical JSON
pub fn to_canonical_json(manifest: &Manifest) -> anyhow::Result<Vec<u8>> {
    CanonicalSerializer::serialize(manifest)
}

/// Convenience function to serialize a manifest to canonical JSON string
pub fn to_canonical_json_string(manifest: &Manifest) -> anyhow::Result<String> {
    let bytes = to_canonical_json(manifest)?;
    Ok(String::from_utf8(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Member, MemberType};

    #[test]
    fn test_canonical_serialization_deterministic() {
        let mut manifest1 = Manifest::new(Some("test manifest".to_string()));
        manifest1.add_member(Member::new(
            "test.json".to_string(),
            "sha256:abc123".to_string(),
            MemberType::Other,
            None,
        ));

        let mut manifest2 = Manifest::new(Some("test manifest".to_string()));
        manifest2.add_member(Member::new(
            "test.json".to_string(),
            "sha256:abc123".to_string(),
            MemberType::Other,
            None,
        ));

        // Set same timestamps to ensure determinism
        manifest1.created = "2026-01-15T10:30:00Z".to_string();
        manifest2.created = "2026-01-15T10:30:00Z".to_string();

        let json1 = to_canonical_json(&manifest1).expect("Failed to serialize manifest1");
        let json2 = to_canonical_json(&manifest2).expect("Failed to serialize manifest2");

        assert_eq!(json1, json2, "Canonical serialization should be deterministic");
    }

    #[test]
    fn test_key_ordering() {
        let manifest = Manifest::new(None);
        let json_bytes = to_canonical_json(&manifest).expect("Failed to serialize");
        let json_str = String::from_utf8(json_bytes).expect("Invalid UTF-8");

        // Parse back to check key ordering
        let value: Value = serde_json::from_str(&json_str).expect("Failed to parse JSON");
        if let Value::Object(map) = value {
            let keys: Vec<&String> = map.keys().collect();
            let mut sorted_keys = keys.clone();
            sorted_keys.sort();
            assert_eq!(keys, sorted_keys, "Keys should be in sorted order");
        }
    }

    #[test]
    fn test_for_hash_computation() {
        let mut manifest = Manifest::new(Some("test".to_string()));
        manifest.set_pack_id("sha256:original".to_string());

        let hash_manifest = manifest.for_hash_computation();

        assert_eq!(hash_manifest.pack_id, "");
        assert_eq!(hash_manifest.note, manifest.note);
        assert_eq!(hash_manifest.version, manifest.version);
    }
}