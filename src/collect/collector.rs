//! Artifact collector for deterministic file gathering

use crate::collect::path::{normalize_member_path, extract_filename, create_member_path};
use crate::refusal::RefusalCode;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a collected file ready for pack inclusion
#[derive(Debug, Clone, PartialEq)]
pub struct CollectedFile {
    /// Source file path on filesystem
    pub source_path: PathBuf,

    /// Member path within the pack (relative, normalized)
    pub member_path: String,
}

/// Artifact collector for deterministic file gathering
pub struct ArtifactCollector {
    /// Map of member paths to collected files (for collision detection)
    files: BTreeMap<String, CollectedFile>,
}

impl ArtifactCollector {
    /// Create a new artifact collector
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    /// Collect artifacts from a list of file/directory paths
    pub fn collect<P: AsRef<Path>>(&mut self, inputs: &[P]) -> Result<(), CollectionError> {
        for input_path in inputs {
            self.collect_input(input_path.as_ref())?;
        }
        Ok(())
    }

    /// Get all collected files in deterministic order
    pub fn get_files(&self) -> Vec<CollectedFile> {
        self.files.values().cloned().collect()
    }

    /// Check if any files were collected
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Get count of collected files
    pub fn count(&self) -> usize {
        self.files.len()
    }

    /// Collect a single input (file or directory)
    fn collect_input(&mut self, input_path: &Path) -> Result<(), CollectionError> {
        if !input_path.exists() {
            return Err(CollectionError::Io {
                path: Some(input_path.to_path_buf()),
                operation: "read".to_string(),
                error: "File or directory does not exist".to_string(),
            });
        }

        let metadata = fs::metadata(input_path).map_err(|e| CollectionError::Io {
            path: Some(input_path.to_path_buf()),
            operation: "stat".to_string(),
            error: e.to_string(),
        })?;

        if metadata.is_file() {
            self.collect_file(input_path, None)?;
        } else if metadata.is_dir() {
            self.collect_directory(input_path)?;
        } else {
            // Symlink, socket, device, FIFO, etc.
            return Err(CollectionError::NonRegularFile {
                path: input_path.to_path_buf(),
                file_type: get_file_type_description(&metadata),
            });
        }

        Ok(())
    }

    /// Collect a single file
    fn collect_file(&mut self, file_path: &Path, dir_context: Option<&str>) -> Result<(), CollectionError> {
        // Determine member path
        let member_path = if let Some(dir_basename) = dir_context {
            // File within a directory - use directory basename + relative path
            let relative_path = extract_filename(file_path)
                .map_err(|e| CollectionError::PathNormalization {
                    path: file_path.to_path_buf(),
                    error: e.to_string()
                })?;
            create_member_path(dir_basename, &relative_path)
        } else {
            // Direct file argument - use basename as member path
            extract_filename(file_path)
                .map_err(|e| CollectionError::PathNormalization {
                    path: file_path.to_path_buf(),
                    error: e.to_string()
                })?
        };

        // Validate member path is safe
        let normalized_member_path = normalize_member_path(&member_path)
            .map_err(|e| CollectionError::PathNormalization {
                path: file_path.to_path_buf(),
                error: e.to_string()
            })?;

        // Check for collision
        if let Some(existing) = self.files.get(&normalized_member_path) {
            return Err(CollectionError::DuplicatePath {
                member_path: normalized_member_path,
                sources: vec![existing.source_path.clone(), file_path.to_path_buf()],
            });
        }

        // Add to collection
        let collected_file = CollectedFile {
            source_path: file_path.to_path_buf(),
            member_path: normalized_member_path.clone(),
        };

        self.files.insert(normalized_member_path, collected_file);

        Ok(())
    }

    /// Collect all files from a directory recursively
    fn collect_directory(&mut self, dir_path: &Path) -> Result<(), CollectionError> {
        let dir_basename = extract_filename(dir_path)
            .map_err(|e| CollectionError::PathNormalization {
                path: dir_path.to_path_buf(),
                error: e.to_string()
            })?;

        // Walk directory recursively
        self.walk_directory(dir_path, &dir_basename, "")?;

        Ok(())
    }

    /// Recursively walk directory and collect files
    fn walk_directory(&mut self, base_dir: &Path, dir_basename: &str, relative_path: &str) -> Result<(), CollectionError> {
        let current_dir = if relative_path.is_empty() {
            base_dir.to_path_buf()
        } else {
            base_dir.join(relative_path)
        };

        let entries = fs::read_dir(&current_dir).map_err(|e| CollectionError::Io {
            path: Some(current_dir.clone()),
            operation: "read_dir".to_string(),
            error: e.to_string(),
        })?;

        // Collect entries and sort for deterministic order
        let mut sorted_entries = Vec::new();
        for entry_result in entries {
            let entry = entry_result.map_err(|e| CollectionError::Io {
                path: Some(current_dir.clone()),
                operation: "read_dir_entry".to_string(),
                error: e.to_string(),
            })?;
            sorted_entries.push(entry);
        }

        // Sort by file name for deterministic processing order
        sorted_entries.sort_by(|a, b| {
            a.file_name().cmp(&b.file_name())
        });

        for entry in sorted_entries {
            let entry_path = entry.path();
            let entry_name = entry.file_name();
            let entry_name_str = entry_name.to_str()
                .ok_or_else(|| CollectionError::PathNormalization {
                    path: entry_path.clone(),
                    error: "Filename contains invalid UTF-8".to_string(),
                })?;

            let new_relative_path = if relative_path.is_empty() {
                entry_name_str.to_string()
            } else {
                format!("{}/{}", relative_path, entry_name_str)
            };

            let metadata = entry.metadata().map_err(|e| CollectionError::Io {
                path: Some(entry_path.clone()),
                operation: "metadata".to_string(),
                error: e.to_string(),
            })?;

            if metadata.is_file() {
                let member_path = create_member_path(dir_basename, &new_relative_path);

                // Check for collision
                if let Some(existing) = self.files.get(&member_path) {
                    return Err(CollectionError::DuplicatePath {
                        member_path: member_path.clone(),
                        sources: vec![existing.source_path.clone(), entry_path.clone()],
                    });
                }

                let collected_file = CollectedFile {
                    source_path: entry_path,
                    member_path: member_path.clone(),
                };

                self.files.insert(member_path, collected_file);

            } else if metadata.is_dir() {
                // Recursively process subdirectory
                self.walk_directory(base_dir, dir_basename, &new_relative_path)?;
            } else {
                // Non-regular file (symlink, socket, device, FIFO)
                return Err(CollectionError::NonRegularFile {
                    path: entry_path,
                    file_type: get_file_type_description(&metadata),
                });
            }
        }

        Ok(())
    }
}

impl Default for ArtifactCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Collection errors mapped to refusal codes
#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    /// IO operation failed
    #[error("IO operation '{operation}' failed on {}: {error}", path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unknown path".to_string()))]
    Io {
        path: Option<PathBuf>,
        operation: String,
        error: String,
    },
    /// Path normalization failed
    #[error("Path normalization failed for {}: {error}", path.display())]
    PathNormalization {
        path: PathBuf,
        error: String,
    },
    /// Duplicate member path collision
    #[error("Duplicate member path '{member_path}' from {} sources", sources.len())]
    DuplicatePath {
        member_path: String,
        sources: Vec<PathBuf>,
    },
    /// Non-regular file encountered
    #[error("Non-regular file ({file_type}) not supported: {}", path.display())]
    NonRegularFile {
        path: PathBuf,
        file_type: String,
    },
}

impl CollectionError {
    /// Convert to refusal code and detail
    pub fn to_refusal(&self) -> (RefusalCode, crate::refusal::RefusalDetail) {
        match self {
            CollectionError::Io { path, operation, error } => {
                RefusalCode::io_error(
                    path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    operation.clone(),
                    error.clone(),
                )
            }
            CollectionError::PathNormalization { path, error } => {
                RefusalCode::io_error(
                    Some(path.to_string_lossy().to_string()),
                    "path_normalization".to_string(),
                    error.clone(),
                )
            }
            CollectionError::DuplicatePath { member_path, sources } => {
                RefusalCode::duplicate(
                    member_path.clone(),
                    sources.iter().map(|p| p.to_string_lossy().to_string()).collect(),
                )
            }
            CollectionError::NonRegularFile { path, file_type: _ } => {
                RefusalCode::io_error(
                    Some(path.to_string_lossy().to_string()),
                    "file_type_check".to_string(),
                    "Non-regular file not supported".to_string(),
                )
            }
        }
    }
}

/// Get human-readable file type description
fn get_file_type_description(metadata: &fs::Metadata) -> String {
    if metadata.is_symlink() {
        "symbolic link"
    } else if metadata.file_type().is_dir() {
        "directory"
    } else if metadata.file_type().is_file() {
        "regular file"
    } else {
        "special file (socket/device/fifo)"
    }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_files() -> anyhow::Result<TempDir> {
        let temp_dir = TempDir::new()?;
        let base = temp_dir.path();

        // Create test files
        fs::write(base.join("test1.txt"), "content1")?;
        fs::write(base.join("test2.json"), r#"{"key": "value"}"#)?;

        // Create subdirectory with files
        fs::create_dir(base.join("subdir"))?;
        fs::write(base.join("subdir/nested.txt"), "nested content")?;
        fs::write(base.join("subdir/data.json"), r#"{"nested": true}"#)?;

        // Create deeper nesting
        fs::create_dir(base.join("subdir/deeper"))?;
        fs::write(base.join("subdir/deeper/deep.txt"), "deep content")?;

        Ok(temp_dir)
    }

    #[test]
    fn test_collect_single_file() -> anyhow::Result<()> {
        let temp_dir = create_test_files()?;
        let base = temp_dir.path();

        let mut collector = ArtifactCollector::new();
        collector.collect(&[base.join("test1.txt")])?;

        let files = collector.get_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].member_path, "test1.txt");
        assert_eq!(files[0].source_path, base.join("test1.txt"));

        Ok(())
    }

    #[test]
    fn test_collect_directory() -> anyhow::Result<()> {
        let temp_dir = create_test_files()?;
        let base = temp_dir.path();

        let mut collector = ArtifactCollector::new();
        collector.collect(&[base.join("subdir")])?;

        let files = collector.get_files();
        assert_eq!(files.len(), 3);

        // Files should be in deterministic order (sorted by member path)
        assert_eq!(files[0].member_path, "subdir/data.json");
        assert_eq!(files[1].member_path, "subdir/deeper/deep.txt");
        assert_eq!(files[2].member_path, "subdir/nested.txt");

        Ok(())
    }

    #[test]
    fn test_collect_multiple_inputs() -> anyhow::Result<()> {
        let temp_dir = create_test_files()?;
        let base = temp_dir.path();

        let mut collector = ArtifactCollector::new();
        collector.collect(&[
            base.join("test1.txt"),
            base.join("test2.json"),
            base.join("subdir")
        ])?;

        let files = collector.get_files();
        assert_eq!(files.len(), 5);

        // Check some specific files
        let member_paths: Vec<_> = files.iter().map(|f| &f.member_path).collect();
        assert!(member_paths.contains(&&"test1.txt".to_string()));
        assert!(member_paths.contains(&&"test2.json".to_string()));
        assert!(member_paths.contains(&&"subdir/data.json".to_string()));
        assert!(member_paths.contains(&&"subdir/nested.txt".to_string()));
        assert!(member_paths.contains(&&"subdir/deeper/deep.txt".to_string()));

        Ok(())
    }

    #[test]
    fn test_duplicate_collision() -> anyhow::Result<()> {
        let temp_dir1 = TempDir::new()?;
        let temp_dir2 = TempDir::new()?;

        // Create files with same name in different locations
        fs::write(temp_dir1.path().join("conflict.txt"), "content1")?;
        fs::write(temp_dir2.path().join("conflict.txt"), "content2")?;

        let mut collector = ArtifactCollector::new();
        let result = collector.collect(&[
            temp_dir1.path().join("conflict.txt"),
            temp_dir2.path().join("conflict.txt"),
        ]);

        match result {
            Err(CollectionError::DuplicatePath { member_path, sources }) => {
                assert_eq!(member_path, "conflict.txt");
                assert_eq!(sources.len(), 2);
            }
            other => panic!("Expected DuplicatePath error, got {:?}", other),
        }

        Ok(())
    }

    #[test]
    fn test_empty_collector() {
        let collector = ArtifactCollector::new();
        assert!(collector.is_empty());
        assert_eq!(collector.count(), 0);
    }

    #[test]
    fn test_nonexistent_file() {
        let mut collector = ArtifactCollector::new();
        let result = collector.collect(&[PathBuf::from("/nonexistent/file.txt")]);

        match result {
            Err(CollectionError::Io { .. }) => {
                // Expected
            }
            other => panic!("Expected Io error, got {:?}", other),
        }
    }
}