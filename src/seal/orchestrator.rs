//! Seal command orchestration logic

use crate::collect::{ArtifactCollector, collector::CollectionError};
use crate::copy::{MemberProcessor, processor::ProcessingError};
use crate::finalize::{ManifestWriter, writer::WriterError};
use crate::refusal::RefusalCode;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Seal orchestrator for end-to-end pack creation
pub struct SealOrchestrator {
    /// Input artifacts to seal
    artifacts: Vec<PathBuf>,

    /// Output directory (final destination)
    output_dir: PathBuf,

    /// Optional annotation
    note: Option<String>,
}

impl SealOrchestrator {
    /// Create a new seal orchestrator
    pub fn new<P: AsRef<Path>>(
        artifacts: Vec<P>,
        output_dir: P,
        note: Option<String>,
    ) -> Self {
        Self {
            artifacts: artifacts.into_iter().map(|p| p.as_ref().to_path_buf()).collect(),
            output_dir: output_dir.as_ref().to_path_buf(),
            note,
        }
    }

    /// Execute the complete seal operation
    pub fn seal(&self) -> Result<SealResult, SealError> {
        // Step 1: Check if output directory is non-empty (refuse if so)
        if self.output_dir.exists() && !self.is_directory_empty(&self.output_dir)? {
            return Err(SealError::OutputNotEmpty {
                output_dir: self.output_dir.clone(),
            });
        }

        // Step 2: Create staging directory for atomic operation
        let staging_dir = TempDir::new().map_err(|e| SealError::Io {
            path: None,
            operation: "create_staging_dir".to_string(),
            error: e.to_string(),
        })?;

        let staging_path = staging_dir.path();

        // Step 3: Collect artifacts (includes collision detection)
        let mut collector = ArtifactCollector::new();
        collector.collect(&self.artifacts).map_err(SealError::Collection)?;

        if collector.is_empty() {
            // This should have been caught by CLI validation, but double-check
            return Err(SealError::NoArtifacts);
        }

        let collected_files = collector.get_files();

        // Step 4: Copy and hash members
        let processor = MemberProcessor::new(staging_path);
        processor.ensure_output_dir().map_err(SealError::Processing)?;

        let processed_members = processor.process_members(&collected_files).map_err(SealError::Processing)?;

        // Step 5: Finalize manifest with pack_id computation
        let writer = ManifestWriter::new(staging_path);
        let finalized = writer.finalize_and_write(&processed_members, self.note.clone()).map_err(SealError::Writer)?;

        // Step 6: Atomic promotion - move staging to final output
        self.atomic_promotion(staging_path, &self.output_dir)?;

        // Don't drop staging_dir until after promotion
        drop(staging_dir);

        Ok(SealResult {
            pack_id: finalized.pack_id().to_string(),
            output_dir: self.output_dir.clone(),
            member_count: finalized.member_count(),
        })
    }

    /// Check if directory is empty or doesn't exist
    fn is_directory_empty(&self, dir: &Path) -> Result<bool, SealError> {
        if !dir.exists() {
            return Ok(true);
        }

        let entries = fs::read_dir(dir).map_err(|e| SealError::Io {
            path: Some(dir.to_path_buf()),
            operation: "read_dir".to_string(),
            error: e.to_string(),
        })?;

        for entry in entries {
            let _entry = entry.map_err(|e| SealError::Io {
                path: Some(dir.to_path_buf()),
                operation: "read_dir_entry".to_string(),
                error: e.to_string(),
            })?;
            return Ok(false); // Found at least one entry
        }

        Ok(true)
    }

    /// Atomic promotion from staging to final output
    fn atomic_promotion(&self, staging_path: &Path, final_path: &Path) -> Result<(), SealError> {
        // Create parent directory if needed
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(|e| SealError::Io {
                path: Some(parent.to_path_buf()),
                operation: "create_dir_all".to_string(),
                error: e.to_string(),
            })?;
        }

        // Atomic rename/move operation
        fs::rename(staging_path, final_path).map_err(|e| SealError::Io {
            path: Some(final_path.to_path_buf()),
            operation: "atomic_rename".to_string(),
            error: e.to_string(),
        })?;

        Ok(())
    }
}

/// Result of successful seal operation
#[derive(Debug, Clone)]
pub struct SealResult {
    /// Computed pack_id
    pub pack_id: String,

    /// Final output directory
    pub output_dir: PathBuf,

    /// Number of members sealed
    pub member_count: usize,
}

impl SealResult {
    /// Generate human-readable success message
    pub fn to_human_output(&self) -> String {
        format!(
            "✓ Pack created successfully\nPack ID: {}\nOutput: {}\nMembers: {}\n",
            self.pack_id,
            self.output_dir.display(),
            self.member_count
        )
    }
}

/// Seal operation errors
#[derive(Debug)]
pub enum SealError {
    /// No artifacts provided (should be caught earlier)
    NoArtifacts,

    /// Output directory is not empty
    OutputNotEmpty {
        output_dir: PathBuf,
    },

    /// Artifact collection error
    Collection(CollectionError),

    /// Member processing error
    Processing(ProcessingError),

    /// Manifest writer error
    Writer(WriterError),

    /// IO operation failed
    Io {
        path: Option<PathBuf>,
        operation: String,
        error: String,
    },
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealError::NoArtifacts => write!(f, "No artifacts provided to seal"),
            SealError::OutputNotEmpty { output_dir } => {
                write!(f, "Output directory is not empty: {}", output_dir.display())
            }
            SealError::Collection(e) => write!(f, "Collection error: {}", e),
            SealError::Processing(e) => write!(f, "Processing error: {}", e),
            SealError::Writer(e) => write!(f, "Writer error: {}", e),
            SealError::Io { path, operation, error } => {
                let path_str = path.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "unknown path".to_string());
                write!(f, "IO operation '{}' failed on {}: {}", operation, path_str, error)
            }
        }
    }
}

impl std::error::Error for SealError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SealError::Collection(e) => Some(e),
            SealError::Processing(e) => Some(e),
            SealError::Writer(e) => Some(e),
            _ => None,
        }
    }
}

impl SealError {
    /// Convert to refusal code and detail for CLI output
    pub fn to_refusal(&self) -> (RefusalCode, crate::refusal::RefusalDetail) {
        match self {
            SealError::NoArtifacts => RefusalCode::empty(),
            SealError::OutputNotEmpty { output_dir } => {
                RefusalCode::io_error(
                    Some(output_dir.to_string_lossy().to_string()),
                    "output_directory_check".to_string(),
                    "Output directory is not empty".to_string(),
                )
            }
            SealError::Collection(e) => e.to_refusal(),
            SealError::Processing(e) => e.to_refusal(),
            SealError::Writer(e) => e.to_refusal(),
            SealError::Io { path, operation, error } => {
                RefusalCode::io_error(
                    path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    operation.clone(),
                    error.clone(),
                )
            }
        }
    }

    /// Get exit code for this error (always REFUSAL = 2)
    pub fn exit_code(&self) -> u8 {
        2
    }
}

/// Convenience function for sealing artifacts
pub fn seal_artifacts<P: AsRef<Path>>(
    artifacts: Vec<P>,
    output_dir: P,
    note: Option<String>,
) -> Result<SealResult, SealError> {
    let orchestrator = SealOrchestrator::new(artifacts, output_dir, note);
    orchestrator.seal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) -> anyhow::Result<PathBuf> {
        let file_path = dir.join(name);
        fs::write(&file_path, content)?;
        Ok(file_path)
    }

    #[test]
    fn test_seal_orchestrator_basic() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let input_dir = temp_dir.path().join("input");
        let output_dir = temp_dir.path().join("output");

        fs::create_dir(&input_dir)?;

        // Create test files
        let file1 = create_test_file(&input_dir, "test1.txt", "content1")?;
        let file2 = create_test_file(&input_dir, "test2.json", r#"{"data": "value"}"#)?;

        // Run seal operation
        let result = seal_artifacts(
            vec![&file1, &file2],
            &output_dir,
            Some("Test seal operation".to_string()),
        )?;

        // Verify results
        assert!(result.pack_id.starts_with("sha256:"));
        assert_eq!(result.member_count, 2);
        assert_eq!(result.output_dir, output_dir);

        // Verify output directory exists
        assert!(output_dir.exists());

        // Verify manifest.json was created
        let manifest_path = output_dir.join("manifest.json");
        assert!(manifest_path.exists());

        // Verify member files were copied
        assert!(output_dir.join("test1.txt").exists());
        assert!(output_dir.join("test2.json").exists());

        Ok(())
    }

    #[test]
    fn test_seal_orchestrator_non_empty_output() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let input_dir = temp_dir.path().join("input");
        let output_dir = temp_dir.path().join("output");

        fs::create_dir_all(&input_dir)?;
        fs::create_dir_all(&output_dir)?;

        // Create a file in output directory (making it non-empty)
        fs::write(output_dir.join("existing.txt"), "already exists")?;

        // Create input file
        let input_file = create_test_file(&input_dir, "test.txt", "content")?;

        // Seal operation should fail
        let result = seal_artifacts(vec![&input_file], &output_dir, None);

        match result {
            Err(SealError::OutputNotEmpty { .. }) => {
                // Expected
            }
            other => panic!("Expected OutputNotEmpty error, got {:?}", other),
        }

        Ok(())
    }

    #[test]
    fn test_seal_orchestrator_empty_output_directory() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let input_dir = temp_dir.path().join("input");
        let output_dir = temp_dir.path().join("output");

        fs::create_dir_all(&input_dir)?;
        fs::create_dir_all(&output_dir)?; // Empty directory should be OK

        let input_file = create_test_file(&input_dir, "test.txt", "content")?;

        // This should succeed
        let result = seal_artifacts(vec![&input_file], &output_dir, None)?;

        assert_eq!(result.member_count, 1);
        assert!(output_dir.join("manifest.json").exists());

        Ok(())
    }

    #[test]
    fn test_seal_result_human_output() {
        let result = SealResult {
            pack_id: "sha256:abc123".to_string(),
            output_dir: PathBuf::from("/path/to/pack"),
            member_count: 3,
        };

        let output = result.to_human_output();
        assert!(output.contains("Pack created successfully"));
        assert!(output.contains("sha256:abc123"));
        assert!(output.contains("/path/to/pack"));
        assert!(output.contains("3"));
    }
}