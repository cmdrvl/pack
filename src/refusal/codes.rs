//! Refusal codes and detail payloads

use serde::{Deserialize, Serialize};

/// Refusal codes for pack operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RefusalCode {
    /// No artifacts provided to seal command
    #[serde(rename = "E_EMPTY")]
    Empty,

    /// Cannot read input, write output, or read pack directory
    #[serde(rename = "E_IO")]
    Io,

    /// Member path collision during seal
    #[serde(rename = "E_DUPLICATE")]
    Duplicate,

    /// Missing/invalid manifest.json for verify/diff/push
    #[serde(rename = "E_BAD_PACK")]
    BadPack,
}

/// Refusal detail payload containing contextual information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RefusalDetail {
    /// Empty refusal - no artifacts provided
    Empty {
        expected: String,
    },

    /// IO error details
    Io {
        path: Option<String>,
        operation: String,
        error: String,
    },

    /// Duplicate path collision
    Duplicate {
        path: String,
        sources: Vec<String>,
    },

    /// Bad pack details
    BadPack {
        pack_dir: String,
        issue: String,
    },
}

impl RefusalCode {
    /// Get human-readable message for the refusal code
    pub fn message(&self) -> &'static str {
        match self {
            RefusalCode::Empty => "No artifacts provided to seal",
            RefusalCode::Io => "IO operation failed",
            RefusalCode::Duplicate => "Resolved member path collision",
            RefusalCode::BadPack => "Invalid pack directory",
        }
    }

    /// Get suggested next command or action
    pub fn next_command(&self) -> Option<String> {
        match self {
            RefusalCode::Empty => Some("Provide files/directories to seal".to_string()),
            RefusalCode::Io => Some("Check paths/permissions".to_string()),
            RefusalCode::Duplicate => Some("Rename inputs or adjust source layout".to_string()),
            RefusalCode::BadPack => Some("Recreate pack via `pack seal`".to_string()),
        }
    }

    /// Create empty refusal
    pub fn empty() -> (Self, RefusalDetail) {
        (
            RefusalCode::Empty,
            RefusalDetail::Empty {
                expected: "files or directories".to_string(),
            }
        )
    }

    /// Create IO error refusal
    pub fn io_error<S: Into<String>>(path: Option<S>, operation: S, error: S) -> (Self, RefusalDetail) {
        (
            RefusalCode::Io,
            RefusalDetail::Io {
                path: path.map(|p| p.into()),
                operation: operation.into(),
                error: error.into(),
            }
        )
    }

    /// Create duplicate path refusal
    pub fn duplicate<S: Into<String>>(path: S, sources: Vec<S>) -> (Self, RefusalDetail) {
        (
            RefusalCode::Duplicate,
            RefusalDetail::Duplicate {
                path: path.into(),
                sources: sources.into_iter().map(|s| s.into()).collect(),
            }
        )
    }

    /// Create bad pack refusal
    pub fn bad_pack<S: Into<String>>(pack_dir: S, issue: S) -> (Self, RefusalDetail) {
        (
            RefusalCode::BadPack,
            RefusalDetail::BadPack {
                pack_dir: pack_dir.into(),
                issue: issue.into(),
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_refusal_code_serialization() {
        assert_eq!(
            serde_json::to_string(&RefusalCode::Empty).unwrap(),
            "\"E_EMPTY\""
        );
        assert_eq!(
            serde_json::to_string(&RefusalCode::Duplicate).unwrap(),
            "\"E_DUPLICATE\""
        );
    }

    #[test]
    fn test_refusal_detail_duplicate() {
        let (code, detail) = RefusalCode::duplicate(
            "test.txt",
            vec!["/path/a/test.txt", "/path/b/test.txt"]
        );

        assert_eq!(code, RefusalCode::Duplicate);
        match detail {
            RefusalDetail::Duplicate { path, sources } => {
                assert_eq!(path, "test.txt");
                assert_eq!(sources, vec!["/path/a/test.txt", "/path/b/test.txt"]);
            }
            _ => panic!("Expected Duplicate detail"),
        }
    }

    #[test]
    fn test_refusal_detail_io() {
        let (code, detail) = RefusalCode::io_error(
            Some("/path/to/file"),
            "read",
            "Permission denied"
        );

        assert_eq!(code, RefusalCode::Io);
        match detail {
            RefusalDetail::Io { path, operation, error } => {
                assert_eq!(path, Some("/path/to/file".to_string()));
                assert_eq!(operation, "read");
                assert_eq!(error, "Permission denied");
            }
            _ => panic!("Expected Io detail"),
        }
    }

    #[test]
    fn test_messages_and_next_commands() {
        assert_eq!(RefusalCode::Empty.message(), "No artifacts provided to seal");
        assert_eq!(
            RefusalCode::Duplicate.next_command(),
            Some("Rename inputs or adjust source layout".to_string())
        );
    }
}