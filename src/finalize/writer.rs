//! Manifest writer with pack_id computation

use crate::manifest::{Manifest, Member, to_canonical_json};
use crate::copy::ProcessedMember;
use crate::copy::hasher::hash_bytes;
use crate::refusal::RefusalCode;
use std::fs;
use std::path::{Path, PathBuf};

/// Manifest writer for computing pack_id and writing final manifest
pub struct ManifestWriter {
    /// Output directory where manifest.json will be written
    output_dir: PathBuf,
}

impl ManifestWriter {
    /// Create a new manifest writer
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
        }
    }

    /// Finalize manifest with pack_id computation and write to output directory
    pub fn finalize_and_write(
        &self,
        processed_members: &[ProcessedMember],
        note: Option<String>,
    ) -> Result<FinalizedManifest, WriterError> {
        // Build initial manifest with members
        let mut manifest = Manifest::new(note);

        // Add all members to manifest (they'll be sorted automatically)
        for processed_member in processed_members {
            let member = processed_member.to_manifest_member();
            manifest.add_member(member);
        }

        // Compute pack_id using self-hash procedure
        let pack_id = self.compute_pack_id(&manifest)?;

        // Set the computed pack_id
        manifest.set_pack_id(pack_id.clone());

        // Write final manifest to disk
        self.write_manifest(&manifest)?;

        Ok(FinalizedManifest {
            manifest,
            manifest_path: self.output_dir.join("manifest.json"),
        })
    }

    /// Compute pack_id using canonical JSON self-hash procedure
    fn compute_pack_id(&self, manifest: &Manifest) -> Result<String, WriterError> {
        // Get manifest with pack_id="" for hash computation
        let hash_manifest = manifest.for_hash_computation();

        // Serialize to canonical JSON
        let canonical_bytes = to_canonical_json(&hash_manifest).map_err(|e| WriterError::Serialization {
            error: e.to_string(),
        })?;

        // Compute SHA256 hash of canonical bytes
        let pack_id = hash_bytes(&canonical_bytes);

        Ok(pack_id)
    }

    /// Write manifest.json to output directory
    fn write_manifest(&self, manifest: &Manifest) -> Result<(), WriterError> {
        let manifest_path = self.output_dir.join("manifest.json");

        // Serialize manifest to pretty JSON for final output
        let json_content = serde_json::to_string_pretty(manifest).map_err(|e| WriterError::Serialization {
            error: e.to_string(),
        })?;

        // Write to file
        fs::write(&manifest_path, json_content).map_err(|e| WriterError::Io {
            path: Some(manifest_path),
            operation: "write".to_string(),
            error: e.to_string(),
        })?;

        Ok(())
    }

    /// Verify pack_id computation matches our algorithm (for testing)
    pub fn verify_pack_id(&self, manifest: &Manifest) -> Result<bool, WriterError> {
        let computed_pack_id = self.compute_pack_id(manifest)?;
        Ok(computed_pack_id == manifest.pack_id)
    }
}

/// Result of manifest finalization
#[derive(Debug, Clone)]
pub struct FinalizedManifest {
    /// The finalized manifest with computed pack_id
    pub manifest: Manifest,

    /// Path where manifest.json was written
    pub manifest_path: PathBuf,
}

impl FinalizedManifest {
    /// Get the computed pack_id
    pub fn pack_id(&self) -> &str {
        &self.manifest.pack_id
    }

    /// Get the member count
    pub fn member_count(&self) -> usize {
        self.manifest.member_count
    }

    /// Get all members
    pub fn members(&self) -> &[Member] {
        &self.manifest.members
    }
}

/// Writer errors
#[derive(Debug)]
pub enum WriterError {
    /// IO operation failed
    Io {
        path: Option<PathBuf>,
        operation: String,
        error: String,
    },
    /// JSON serialization failed
    Serialization {
        error: String,
    },
}

impl std::fmt::Display for WriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriterError::Io { path, operation, error } => {
                let path_str = path.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "unknown path".to_string());
                write!(f, "IO operation '{}' failed on {}: {}", operation, path_str, error)
            }
            WriterError::Serialization { error } => {
                write!(f, "JSON serialization failed: {}", error)
            }
        }
    }
}

impl std::error::Error for WriterError {}

impl WriterError {
    /// Convert to refusal code and detail
    pub fn to_refusal(&self) -> (RefusalCode, crate::refusal::RefusalDetail) {
        match self {
            WriterError::Io { path, operation, error } => {
                RefusalCode::io_error(
                    path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    operation.clone(),
                    error.clone(),
                )
            }
            WriterError::Serialization { error } => {
                RefusalCode::io_error(
                    Some("manifest.json".to_string()),
                    "serialization".to_string(),
                    error.clone(),
                )
            }
        }
    }
}

/// Convenience function for manifest finalization
pub fn finalize_manifest<P: AsRef<Path>>(
    output_dir: P,
    processed_members: &[ProcessedMember],
    note: Option<String>,
) -> Result<FinalizedManifest, WriterError> {
    let writer = ManifestWriter::new(output_dir);
    writer.finalize_and_write(processed_members, note)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::CollectedFile;
    use crate::manifest::MemberType;
    use tempfile::{TempDir, NamedTempFile};
    use std::io::Write;

    fn create_test_processed_member(
        content: &str,
        member_path: &str,
        hash: Option<String>,
    ) -> anyhow::Result<ProcessedMember> {
        let mut temp_file = NamedTempFile::new()?;
        write!(temp_file, "{}", content)?;

        let collected_file = CollectedFile {
            source_path: temp_file.path().to_path_buf(),
            member_path: member_path.to_string(),
        };

        let bytes_hash = hash.unwrap_or_else(|| crate::copy::hasher::hash_bytes(content.as_bytes()));

        Ok(ProcessedMember {
            collected_file,
            destination_path: PathBuf::from("dummy"), // Not used in these tests
            bytes_hash,
            member_type: MemberType::Other,
            artifact_version: None,
        })
    }

    #[test]
    fn test_manifest_writer_basic() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let writer = ManifestWriter::new(temp_dir.path());

        let processed_member = create_test_processed_member("test content", "test.txt", None)?;
        let finalized = writer.finalize_and_write(&[processed_member], Some("test note".to_string()))?;

        // Check manifest properties
        assert_eq!(finalized.manifest.version, "pack.v0");
        assert!(finalized.pack_id().starts_with("sha256:"));
        assert_eq!(finalized.member_count(), 1);
        assert_eq!(finalized.manifest.note, Some("test note".to_string()));

        // Check member
        let members = finalized.members();
        assert_eq!(members[0].path, "test.txt");
        assert_eq!(members[0].member_type, MemberType::Other);

        // Check manifest.json was written
        assert!(finalized.manifest_path.exists());

        let written_content = fs::read_to_string(&finalized.manifest_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&written_content)?;
        assert_eq!(parsed["version"], "pack.v0");
        assert_eq!(parsed["member_count"], 1);

        Ok(())
    }

    #[test]
    fn test_pack_id_computation_deterministic() -> anyhow::Result<()> {
        let temp_dir1 = TempDir::new()?;
        let temp_dir2 = TempDir::new()?;

        let writer1 = ManifestWriter::new(temp_dir1.path());
        let writer2 = ManifestWriter::new(temp_dir2.path());

        let processed_member = create_test_processed_member("identical content", "same.txt", None)?;

        // Use same timestamp to ensure determinism
        let finalized1 = writer1.finalize_and_write(&[processed_member.clone()], Some("same note".to_string()))?;

        // Manually set same created timestamp for second manifest
        let mut manifest2 = Manifest::new(Some("same note".to_string()));
        manifest2.created = finalized1.manifest.created.clone(); // Use same timestamp
        manifest2.add_member(processed_member.to_manifest_member());

        let pack_id2 = writer2.compute_pack_id(&manifest2)?;

        // pack_ids should be identical for identical inputs
        assert_eq!(finalized1.pack_id(), pack_id2);

        Ok(())
    }

    #[test]
    fn test_pack_id_self_hash_contract() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let writer = ManifestWriter::new(temp_dir.path());

        let processed_member = create_test_processed_member("test", "file.txt", None)?;
        let finalized = writer.finalize_and_write(&[processed_member], None)?;

        // Verify that recomputing the pack_id gives the same result
        assert!(writer.verify_pack_id(&finalized.manifest)?);

        // Verify that the pack_id computation used manifest with pack_id=""
        let hash_manifest = finalized.manifest.for_hash_computation();
        assert_eq!(hash_manifest.pack_id, "");

        let canonical_bytes = to_canonical_json(&hash_manifest)?;
        let recomputed_pack_id = hash_bytes(&canonical_bytes);
        assert_eq!(recomputed_pack_id, finalized.pack_id());

        Ok(())
    }

    #[test]
    fn test_multiple_members_sorted() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let writer = ManifestWriter::new(temp_dir.path());

        let member1 = create_test_processed_member("content1", "z_last.txt", None)?;
        let member2 = create_test_processed_member("content2", "a_first.txt", None)?;
        let member3 = create_test_processed_member("content3", "m_middle.txt", None)?;

        let finalized = writer.finalize_and_write(&[member1, member2, member3], None)?;

        assert_eq!(finalized.member_count(), 3);
        let members = finalized.members();

        // Members should be sorted by path
        assert_eq!(members[0].path, "a_first.txt");
        assert_eq!(members[1].path, "m_middle.txt");
        assert_eq!(members[2].path, "z_last.txt");

        Ok(())
    }

    #[test]
    fn test_member_count_consistency() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let writer = ManifestWriter::new(temp_dir.path());

        let members: Vec<_> = (0..5)
            .map(|i| create_test_processed_member(&format!("content{}", i), &format!("file{}.txt", i), None))
            .collect::<Result<_, _>>()?;

        let finalized = writer.finalize_and_write(&members, None)?;

        // member_count should equal actual member count
        assert_eq!(finalized.member_count(), members.len());
        assert_eq!(finalized.manifest.member_count, finalized.members().len());

        Ok(())
    }

    #[test]
    fn test_convenience_function() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let processed_member = create_test_processed_member("test", "test.txt", None)?;
        let finalized = finalize_manifest(temp_dir.path(), &[processed_member], Some("via convenience".to_string()))?;

        assert_eq!(finalized.manifest.note, Some("via convenience".to_string()));
        assert!(finalized.manifest_path.exists());

        Ok(())
    }
}