use serde::{Deserialize, Serialize};

use super::RefusalCode;

/// Detail payload within a refusal envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefusalDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
    pub next_command: Option<String>,
}

/// The full refusal envelope emitted on stdout (exit 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefusalEnvelope {
    pub version: String,
    pub outcome: String,
    pub refusal: RefusalDetail,
}

impl RefusalEnvelope {
    /// Build a refusal envelope from a code, optional message override, and optional detail.
    pub fn new(
        code: RefusalCode,
        message: Option<String>,
        detail: Option<serde_json::Value>,
    ) -> Self {
        Self {
            version: "pack.v0".to_string(),
            outcome: "REFUSAL".to_string(),
            refusal: RefusalDetail {
                code: code.as_str().to_string(),
                message: message.unwrap_or_else(|| code.default_message().to_string()),
                detail,
                next_command: None,
            },
        }
    }

    /// Serialize to deterministic JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("refusal envelope serialization cannot fail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_refusal_envelope() {
        let env = RefusalEnvelope::new(RefusalCode::Empty, None, None);
        assert_eq!(env.version, "pack.v0");
        assert_eq!(env.outcome, "REFUSAL");
        assert_eq!(env.refusal.code, "E_EMPTY");
        assert_eq!(env.refusal.message, "No artifacts provided to seal");
        assert!(env.refusal.detail.is_none());
        assert!(env.refusal.next_command.is_none());
    }

    #[test]
    fn io_refusal_with_custom_message() {
        let env = RefusalEnvelope::new(
            RefusalCode::Io,
            Some("Cannot read /foo/bar".to_string()),
            None,
        );
        assert_eq!(env.refusal.code, "E_IO");
        assert_eq!(env.refusal.message, "Cannot read /foo/bar");
    }

    #[test]
    fn duplicate_refusal_with_detail() {
        let detail = json!({
            "path": "nov.lock.json",
            "sources": ["/a/nov.lock.json", "/b/nov.lock.json"]
        });
        let env = RefusalEnvelope::new(RefusalCode::Duplicate, None, Some(detail.clone()));
        assert_eq!(env.refusal.code, "E_DUPLICATE");
        assert_eq!(env.refusal.detail, Some(detail));
    }

    #[test]
    fn bad_pack_refusal() {
        let env = RefusalEnvelope::new(RefusalCode::BadPack, None, None);
        assert_eq!(env.refusal.code, "E_BAD_PACK");
        assert_eq!(env.refusal.message, "Missing or invalid manifest.json");
    }

    #[test]
    fn to_json_roundtrips() {
        let env = RefusalEnvelope::new(RefusalCode::Empty, None, None);
        let json_str = env.to_json();
        let parsed: RefusalEnvelope = serde_json::from_str(&json_str).unwrap();
        assert_eq!(env, parsed);
    }

    #[test]
    fn envelope_has_correct_shape() {
        let env = RefusalEnvelope::new(RefusalCode::Duplicate, None, Some(json!({"path": "x"})));
        let json_str = env.to_json();
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(val["version"], "pack.v0");
        assert_eq!(val["outcome"], "REFUSAL");
        assert_eq!(val["refusal"]["code"], "E_DUPLICATE");
        assert!(val["refusal"]["message"].is_string());
        assert_eq!(val["refusal"]["detail"]["path"], "x");
        assert!(val["refusal"]["next_command"].is_null());
    }

    #[test]
    fn all_codes_have_deterministic_strings() {
        let codes = [
            (RefusalCode::Empty, "E_EMPTY"),
            (RefusalCode::Io, "E_IO"),
            (RefusalCode::Duplicate, "E_DUPLICATE"),
            (RefusalCode::BadPack, "E_BAD_PACK"),
        ];
        for (code, expected) in &codes {
            assert_eq!(code.as_str(), *expected);
            assert!(!code.default_message().is_empty());
            assert_eq!(format!("{code}"), *expected);
        }
    }
}
