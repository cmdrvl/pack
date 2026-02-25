//! Member copy and processing logic

use crate::collect::CollectedFile;
use crate::copy::hasher::compute_sha256_hex;
use crate::manifest::{Member, MemberType};
use crate::refusal::RefusalCode;
use std::fs;
use std::path::{Path, PathBuf};

/// A processed member with copy and hash information
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedMember {
    /// Original collected file info
    pub collected_file: CollectedFile,

    /// Destination path where member was copied
    pub destination_path: PathBuf,

    /// SHA256 hash of the copied bytes
    pub bytes_hash: String,

    /// Detected member type
    pub member_type: MemberType,

    /// Parsed artifact version (if detected)
    pub artifact_version: Option<String>,
}

impl ProcessedMember {
    /// Convert to manifest Member
    pub fn to_manifest_member(&self) -> Member {
        Member::new(
            self.collected_file.member_path.clone(),
            self.bytes_hash.clone(),
            self.member_type.clone(),
            self.artifact_version.clone(),
        )
    }
}

/// Member processor for copying and hashing files
pub struct MemberProcessor {
    /// Output directory where members are copied
    output_dir: PathBuf,

    /// Whether to create directories as needed
    create_dirs: bool,
}

impl MemberProcessor {
    /// Create a new member processor
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            create_dirs: true,
        }
    }

    /// Process a list of collected files
    pub fn process_members(&self, collected_files: &[CollectedFile]) -> Result<Vec<ProcessedMember>, ProcessingError> {
        let mut processed = Vec::new();

        for collected_file in collected_files {
            let processed_member = self.process_single_member(collected_file)?;
            processed.push(processed_member);
        }

        Ok(processed)
    }

    /// Process a single collected file
    fn process_single_member(&self, collected_file: &CollectedFile) -> Result<ProcessedMember, ProcessingError> {
        // Determine destination path
        let destination_path = self.output_dir.join(&collected_file.member_path);

        // Create parent directories if needed
        if self.create_dirs {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ProcessingError::Io {
                    path: Some(parent.to_path_buf()),
                    operation: "create_dir_all".to_string(),
                    error: e.to_string(),
                })?;
            }
        }

        // Copy file and compute hash
        let (bytes_hash, member_type, artifact_version) = self.copy_and_analyze_file(
            &collected_file.source_path,
            &destination_path,
            &collected_file.member_path,
        )?;

        Ok(ProcessedMember {
            collected_file: collected_file.clone(),
            destination_path,
            bytes_hash,
            member_type,
            artifact_version,
        })
    }

    /// Copy file and analyze its contents for type detection
    fn copy_and_analyze_file(
        &self,
        source_path: &Path,
        destination_path: &Path,
        member_path: &str,
    ) -> Result<(String, MemberType, Option<String>), ProcessingError> {
        // Read source file
        let source_bytes = fs::read(source_path).map_err(|e| ProcessingError::Io {
            path: Some(source_path.to_path_buf()),
            operation: "read".to_string(),
            error: e.to_string(),
        })?;

        // Compute hash from bytes
        let bytes_hash = crate::copy::hasher::hash_bytes(&source_bytes);

        // Detect member type from bytes
        let member_type = MemberType::detect(member_path, &source_bytes);

        // Try to extract artifact version
        let artifact_version = self.extract_artifact_version(&source_bytes);

        // Write to destination
        fs::write(destination_path, &source_bytes).map_err(|e| ProcessingError::Io {
            path: Some(destination_path.to_path_buf()),
            operation: "write".to_string(),
            error: e.to_string(),
        })?;

        // Verify the copy by re-hashing the destination
        let verify_hash = compute_sha256_hex(destination_path).map_err(|e| ProcessingError::Io {
            path: Some(destination_path.to_path_buf()),
            operation: "verify_hash".to_string(),
            error: e.to_string(),
        })?;

        if bytes_hash != verify_hash {
            return Err(ProcessingError::HashMismatch {
                path: destination_path.to_path_buf(),
                expected: bytes_hash,
                actual: verify_hash,
            });
        }

        Ok((bytes_hash, member_type, artifact_version))
    }

    /// Extract artifact version from file contents if possible
    fn extract_artifact_version(&self, bytes: &[u8]) -> Option<String> {
        // Try to parse as JSON and look for version field
        if let Ok(text) = std::str::from_utf8(bytes) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(version) = value.get("version").and_then(|v| v.as_str()) {
                    return Some(version.to_string());
                }
            }
        }
        None
    }

    /// Ensure output directory exists
    pub fn ensure_output_dir(&self) -> Result<(), ProcessingError> {
        if !self.output_dir.exists() {
            fs::create_dir_all(&self.output_dir).map_err(|e| ProcessingError::Io {
                path: Some(self.output_dir.clone()),
                operation: "create_dir_all".to_string(),
                error: e.to_string(),
            })?;
        }
        Ok(())
    }

    /// Check if output directory is empty
    pub fn is_output_dir_empty(&self) -> Result<bool, ProcessingError> {
        if !self.output_dir.exists() {
            return Ok(true);
        }

        let entries = fs::read_dir(&self.output_dir).map_err(|e| ProcessingError::Io {
            path: Some(self.output_dir.clone()),
            operation: "read_dir".to_string(),
            error: e.to_string(),
        })?;

        for entry in entries {
            let _entry = entry.map_err(|e| ProcessingError::Io {
                path: Some(self.output_dir.clone()),
                operation: "read_dir_entry".to_string(),
                error: e.to_string(),
            })?;
            return Ok(false); // Directory is not empty
        }

        Ok(true)
    }
}

/// Processing errors
#[derive(Debug)]
pub enum ProcessingError {
    /// IO operation failed
    Io {
        path: Option<PathBuf>,
        operation: String,
        error: String,
    },
    /// Hash verification failed after copy
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingError::Io { path, operation, error } => {
                let path_str = path.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "unknown path".to_string());
                write!(f, "IO operation '{}' failed on {}: {}", operation, path_str, error)
            }
            ProcessingError::HashMismatch { path, expected, actual } => {
                write!(f, "Hash mismatch for {}: expected {}, got {}", path.display(), expected, actual)
            }
        }
    }
}

impl std::error::Error for ProcessingError {}

impl ProcessingError {
    /// Convert to refusal code and detail
    pub fn to_refusal(&self) -> (RefusalCode, crate::refusal::RefusalDetail) {
        match self {
            ProcessingError::Io { path, operation, error } => {
                RefusalCode::io_error(
                    path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    operation.clone(),
                    error.clone(),
                )
            }
            ProcessingError::HashMismatch { path, expected, actual } => {
                RefusalCode::io_error(
                    Some(path.to_string_lossy().to_string()),
                    "hash_verification".to_string(),
                    format!("Expected {}, got {}", expected, actual),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{TempDir, NamedTempFile};
    use std::io::Write;
    use crate::collect::CollectedFile;

    fn create_test_collected_file(content: &str, member_path: &str) -> anyhow::Result<(CollectedFile, NamedTempFile)> {
        let mut temp_file = NamedTempFile::new()?;
        write!(temp_file, "{}", content)?;

        let collected_file = CollectedFile {
            source_path: temp_file.path().to_path_buf(),
            member_path: member_path.to_string(),
        };

        Ok((collected_file, temp_file))
    }

    #[test]
    fn test_process_single_member() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("output");

        let processor = MemberProcessor::new(&output_dir);
        processor.ensure_output_dir()?;

        // Create test file
        let (collected_file, _temp_file) = create_test_collected_file("test content", "test.txt")?;

        // Process the member
        let processed = processor.process_single_member(&collected_file)?;

        // Verify results
        assert_eq!(processed.collected_file.member_path, "test.txt");
        assert!(processed.bytes_hash.starts_with("sha256:"));
        assert_eq!(processed.member_type, MemberType::Other);

        // Verify file was copied
        let destination_content = fs::read_to_string(&processed.destination_path)?;
        assert_eq!(destination_content, "test content");

        // Verify hash is correct
        let expected_hash = crate::copy::hasher::hash_bytes(b"test content");
        assert_eq!(processed.bytes_hash, expected_hash);

        Ok(())
    }

    #[test]
    fn test_process_json_with_version() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("output");

        let processor = MemberProcessor::new(&output_dir);
        processor.ensure_output_dir()?;

        // Create JSON file with version
        let json_content = r#"{"version": "test.v1", "data": "value"}"#;
        let (collected_file, _temp_file) = create_test_collected_file(json_content, "data.json")?;

        // Process the member
        let processed = processor.process_single_member(&collected_file)?;

        // Verify artifact version was extracted
        assert_eq!(processed.artifact_version, Some("test.v1".to_string()));
        assert_eq!(processed.member_type, MemberType::Other); // Would be specific type in real usage

        Ok(())
    }

    #[test]
    fn test_process_multiple_members() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("output");

        let processor = MemberProcessor::new(&output_dir);
        processor.ensure_output_dir()?;

        // Create multiple test files
        let (collected1, _temp1) = create_test_collected_file("content 1", "file1.txt")?;
        let (collected2, _temp2) = create_test_collected_file("content 2", "subdir/file2.txt")?;

        let collected_files = vec![collected1, collected2];

        // Process all members
        let processed = processor.process_members(&collected_files)?;

        assert_eq!(processed.len(), 2);

        // Verify first file
        assert_eq!(processed[0].collected_file.member_path, "file1.txt");
        let content1 = fs::read_to_string(&processed[0].destination_path)?;
        assert_eq!(content1, "content 1");

        // Verify second file (in subdirectory)
        assert_eq!(processed[1].collected_file.member_path, "subdir/file2.txt");
        let content2 = fs::read_to_string(&processed[1].destination_path)?;
        assert_eq!(content2, "content 2");

        // Verify subdirectory was created
        assert!(output_dir.join("subdir").exists());

        Ok(())
    }

    #[test]
    fn test_to_manifest_member() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("output");

        let processor = MemberProcessor::new(&output_dir);
        processor.ensure_output_dir()?;

        let (collected_file, _temp_file) = create_test_collected_file("test", "test.txt")?;
        let processed = processor.process_single_member(&collected_file)?;

        let manifest_member = processed.to_manifest_member();

        assert_eq!(manifest_member.path, "test.txt");
        assert_eq!(manifest_member.bytes_hash, processed.bytes_hash);
        assert_eq!(manifest_member.member_type, MemberType::Other);
        assert_eq!(manifest_member.artifact_version, None);

        Ok(())
    }

    #[test]
    fn test_empty_output_dir_check() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let empty_dir = temp_dir.path().join("empty");
        let non_empty_dir = temp_dir.path().join("non_empty");

        // Test nonexistent directory (should be considered empty)
        let processor1 = MemberProcessor::new(&empty_dir);
        assert!(processor1.is_output_dir_empty()?);

        // Test empty directory
        fs::create_dir(&empty_dir)?;
        assert!(processor1.is_output_dir_empty()?);

        // Test non-empty directory
        let processor2 = MemberProcessor::new(&non_empty_dir);
        processor2.ensure_output_dir()?;
        fs::write(non_empty_dir.join("file.txt"), "content")?;
        assert!(!processor2.is_output_dir_empty()?);

        Ok(())
    }
}