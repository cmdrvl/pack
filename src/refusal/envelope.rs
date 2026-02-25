//! Refusal envelope output formatting

use crate::refusal::{RefusalCode, RefusalDetail};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// Overall outcome of a pack operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RefusalOutcome {
    Refusal,
}

/// Complete refusal envelope as specified in pack.v0
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefusalEnvelope {
    /// Always "pack.v0"
    pub version: String,

    /// Always "REFUSAL" for refusal envelopes
    pub outcome: RefusalOutcome,

    /// Refusal details
    pub refusal: RefusalInfo,
}

/// Refusal information block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefusalInfo {
    /// Refusal code
    pub code: RefusalCode,

    /// Human-readable message
    pub message: String,

    /// Contextual detail payload
    pub detail: RefusalDetail,

    /// Optional suggested next command
    pub next_command: Option<String>,
}

impl RefusalEnvelope {
    /// Create a new refusal envelope
    pub fn new(code: RefusalCode, detail: RefusalDetail) -> Self {
        let message = code.message().to_string();
        let next_command = code.next_command();

        Self {
            version: "pack.v0".to_string(),
            outcome: RefusalOutcome::Refusal,
            refusal: RefusalInfo {
                code,
                message,
                detail,
                next_command,
            },
        }
    }

    /// Serialize to JSON bytes
    pub fn to_json_bytes(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    /// Serialize to JSON string
    pub fn to_json_string(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Write to stdout and return exit code 2
    pub fn output_and_exit(&self) -> u8 {
        if let Err(e) = self.write_to_stdout() {
            eprintln!("Failed to output refusal envelope: {}", e);
        }
        2 // REFUSAL exit code
    }

    /// Write envelope to stdout
    fn write_to_stdout(&self) -> anyhow::Result<()> {
        let json = self.to_json_string()?;
        println!("{}", json);
        io::stdout().flush()?;
        Ok(())
    }
}

/// Convenience function to output a refusal and return exit code
pub fn output_refusal(code: RefusalCode, detail: RefusalDetail) -> u8 {
    let envelope = RefusalEnvelope::new(code, detail);
    envelope.output_and_exit()
}

/// Convenience function to output an empty refusal
pub fn output_empty_refusal() -> u8 {
    let (code, detail) = RefusalCode::empty();
    output_refusal(code, detail)
}

/// Convenience function to output an IO refusal
pub fn output_io_refusal<S: Into<String>>(path: Option<S>, operation: S, error: S) -> u8 {
    let (code, detail) = RefusalCode::io_error(path, operation, error);
    output_refusal(code, detail)
}

/// Convenience function to output a duplicate refusal
pub fn output_duplicate_refusal<S: Into<String>>(path: S, sources: Vec<S>) -> u8 {
    let (code, detail) = RefusalCode::duplicate(path, sources);
    output_refusal(code, detail)
}

/// Convenience function to output a bad pack refusal
pub fn output_bad_pack_refusal<S: Into<String>>(pack_dir: S, issue: S) -> u8 {
    let (code, detail) = RefusalCode::bad_pack(pack_dir, issue);
    output_refusal(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_refusal_envelope_structure() {
        let (code, detail) = RefusalCode::empty();
        let envelope = RefusalEnvelope::new(code, detail);

        assert_eq!(envelope.version, "pack.v0");
        assert_eq!(envelope.outcome, RefusalOutcome::Refusal);
        assert_eq!(envelope.refusal.code, RefusalCode::Empty);
        assert_eq!(envelope.refusal.message, "No artifacts provided to seal");
        assert_eq!(
            envelope.refusal.next_command,
            Some("Provide files/directories to seal".to_string())
        );
    }

    #[test]
    fn test_refusal_envelope_serialization() {
        let (code, detail) = RefusalCode::duplicate(
            "test.json",
            vec!["a/test.json", "b/test.json"]
        );
        let envelope = RefusalEnvelope::new(code, detail);

        let json = envelope.to_json_string().expect("Failed to serialize");

        // Parse back to verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON");

        assert_eq!(parsed["version"], "pack.v0");
        assert_eq!(parsed["outcome"], "REFUSAL");
        assert_eq!(parsed["refusal"]["code"], "E_DUPLICATE");
        assert_eq!(parsed["refusal"]["detail"]["path"], "test.json");

        let sources = parsed["refusal"]["detail"]["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0], "a/test.json");
        assert_eq!(sources[1], "b/test.json");
    }

    #[test]
    fn test_outcome_serialization() {
        assert_eq!(
            serde_json::to_string(&RefusalOutcome::Refusal).unwrap(),
            "\"REFUSAL\""
        );
    }

    #[test]
    fn test_envelope_roundtrip() {
        let (code, detail) = RefusalCode::io_error(
            Some("/path/file.txt"),
            "write",
            "Disk full"
        );
        let original = RefusalEnvelope::new(code, detail);

        let json = original.to_json_string().unwrap();
        let parsed: RefusalEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(original, parsed);
    }
}