//! Pack integrity verification logic

use crate::manifest::{Manifest, to_canonical_json};
use crate::copy::hasher::compute_sha256_hex;
use crate::refusal::RefusalCode;
use serde::{Serialize, Deserialize};
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Pack verifier for integrity validation
pub struct PackVerifier {
    /// Pack directory being verified
    pack_dir: PathBuf,
}

impl PackVerifier {
    /// Create a new pack verifier
    pub fn new<P: AsRef<Path>>(pack_dir: P) -> Self {
        Self {
            pack_dir: pack_dir.as_ref().to_path_buf(),
        }
    }

    /// Verify pack integrity
    pub fn verify(&self) -> Result<VerifyResult, VerificationError> {
        // Step 1: Load and parse manifest
        let manifest = self.load_manifest()?;

        // Step 2: Run all integrity checks
        let mut checks = IntegrityChecks::new();
        let mut invalid_findings = Vec::new();

        // Check member_count consistency
        if manifest.member_count != manifest.members.len() {
            invalid_findings.push(InvalidFinding {
                code: "MEMBER_COUNT_MISMATCH".to_string(),
                message: format!("Expected {} members, found {} in manifest", manifest.member_count, manifest.members.len()),
                detail: None,
            });
        } else {
            checks.member_count = true;
        }

        // Check member paths
        self.check_member_paths(&manifest, &mut checks, &mut invalid_findings)?;

        // Check for extra files in pack root
        self.check_extra_files(&manifest, &mut checks, &mut invalid_findings)?;

        // Check member files exist and are regular
        self.check_member_files(&manifest, &mut checks, &mut invalid_findings)?;

        // Verify member hashes
        self.verify_member_hashes(&manifest, &mut checks, &mut invalid_findings)?;

        // Recompute and verify pack_id
        self.verify_pack_id(&manifest, &mut checks, &mut invalid_findings)?;

        // Determine outcome
        let outcome = if invalid_findings.is_empty() {
            VerifyOutcome::Ok
        } else {
            VerifyOutcome::Invalid
        };

        Ok(VerifyResult {
            outcome,
            pack_id: manifest.pack_id.clone(),
            checks,
            invalid_findings,
        })
    }

    /// Load and parse manifest.json
    fn load_manifest(&self) -> Result<Manifest, VerificationError> {
        let manifest_path = self.pack_dir.join("manifest.json");

        if !manifest_path.exists() {
            return Err(VerificationError::BadPack {
                pack_dir: self.pack_dir.clone(),
                issue: "manifest.json not found".to_string(),
            });
        }

        let content = fs::read_to_string(&manifest_path).map_err(|e| VerificationError::Io {
            path: Some(manifest_path.clone()),
            operation: "read".to_string(),
            error: e.to_string(),
        })?;

        let manifest: Manifest = serde_json::from_str(&content).map_err(|e| VerificationError::BadPack {
            pack_dir: self.pack_dir.clone(),
            issue: format!("Invalid manifest JSON: {}", e),
        })?;

        // Verify it's pack.v0
        if manifest.version != "pack.v0" {
            return Err(VerificationError::BadPack {
                pack_dir: self.pack_dir.clone(),
                issue: format!("Expected version 'pack.v0', found '{}'", manifest.version),
            });
        }

        Ok(manifest)
    }

    /// Check member path validity and uniqueness
    fn check_member_paths(
        &self,
        manifest: &Manifest,
        checks: &mut IntegrityChecks,
        invalid_findings: &mut Vec<InvalidFinding>,
    ) -> Result<(), VerificationError> {
        let mut seen_paths = HashSet::new();
        let mut paths_valid = true;

        for member in &manifest.members {
            // Check for duplicates
            if seen_paths.contains(&member.path) {
                invalid_findings.push(InvalidFinding {
                    code: "DUPLICATE_MEMBER_PATH".to_string(),
                    message: format!("Duplicate member path: {}", member.path),
                    detail: Some(json!({"path": member.path})),
                });
                paths_valid = false;
            }
            seen_paths.insert(&member.path);

            // Check for reserved path
            if member.path == "manifest.json" {
                invalid_findings.push(InvalidFinding {
                    code: "RESERVED_MEMBER_PATH".to_string(),
                    message: "Reserved member path 'manifest.json' not allowed".to_string(),
                    detail: Some(json!({"path": member.path})),
                });
                paths_valid = false;
            }

            // Check for unsafe paths (absolute or with ..)
            if !self.is_safe_relative_path(&member.path) {
                invalid_findings.push(InvalidFinding {
                    code: "UNSAFE_MEMBER_PATH".to_string(),
                    message: format!("Unsafe member path: {}", member.path),
                    detail: Some(json!({"path": member.path})),
                });
                paths_valid = false;
            }
        }

        checks.member_paths = paths_valid;
        Ok(())
    }

    /// Check for extra files in pack root
    fn check_extra_files(
        &self,
        manifest: &Manifest,
        checks: &mut IntegrityChecks,
        invalid_findings: &mut Vec<InvalidFinding>,
    ) -> Result<(), VerificationError> {
        let entries = fs::read_dir(&self.pack_dir).map_err(|e| VerificationError::Io {
            path: Some(self.pack_dir.clone()),
            operation: "read_dir".to_string(),
            error: e.to_string(),
        })?;

        let mut expected_files: HashSet<String> = manifest.members.iter()
            .map(|m| m.path.clone())
            .collect();
        expected_files.insert("manifest.json".to_string());

        let mut extra_files_found = false;

        for entry in entries {
            let entry = entry.map_err(|e| VerificationError::Io {
                path: Some(self.pack_dir.clone()),
                operation: "read_dir_entry".to_string(),
                error: e.to_string(),
            })?;

            let file_name = entry.file_name().to_string_lossy().to_string();

            if !expected_files.contains(&file_name) {
                invalid_findings.push(InvalidFinding {
                    code: "EXTRA_MEMBER".to_string(),
                    message: format!("Unexpected file in pack: {}", file_name),
                    detail: Some(json!({"path": file_name})),
                });
                extra_files_found = true;
            }
        }

        checks.extra_members = !extra_files_found;
        Ok(())
    }

    /// Check member files exist and are regular files
    fn check_member_files(
        &self,
        manifest: &Manifest,
        checks: &mut IntegrityChecks,
        invalid_findings: &mut Vec<InvalidFinding>,
    ) -> Result<(), VerificationError> {
        let mut all_members_valid = true;

        for member in &manifest.members {
            let member_path = self.pack_dir.join(&member.path);

            if !member_path.exists() {
                invalid_findings.push(InvalidFinding {
                    code: "MISSING_MEMBER".to_string(),
                    message: format!("Missing member file: {}", member.path),
                    detail: Some(json!({"path": member.path})),
                });
                all_members_valid = false;
                continue;
            }

            let metadata = fs::metadata(&member_path).map_err(|e| VerificationError::Io {
                path: Some(member_path.clone()),
                operation: "metadata".to_string(),
                error: e.to_string(),
            })?;

            if !metadata.is_file() {
                invalid_findings.push(InvalidFinding {
                    code: "NON_REGULAR_MEMBER".to_string(),
                    message: format!("Member is not a regular file: {}", member.path),
                    detail: Some(json!({"path": member.path})),
                });
                all_members_valid = false;
            }
        }

        checks.member_files = all_members_valid;
        Ok(())
    }

    /// Verify member hashes
    fn verify_member_hashes(
        &self,
        manifest: &Manifest,
        checks: &mut IntegrityChecks,
        invalid_findings: &mut Vec<InvalidFinding>,
    ) -> Result<(), VerificationError> {
        let mut all_hashes_valid = true;

        for member in &manifest.members {
            let member_path = self.pack_dir.join(&member.path);

            if !member_path.exists() {
                // Already reported in check_member_files
                all_hashes_valid = false;
                continue;
            }

            let actual_hash = compute_sha256_hex(&member_path).map_err(|e| VerificationError::Io {
                path: Some(member_path),
                operation: "hash_computation".to_string(),
                error: e.to_string(),
            })?;

            if actual_hash != member.bytes_hash {
                invalid_findings.push(InvalidFinding {
                    code: "HASH_MISMATCH".to_string(),
                    message: format!("Hash mismatch for member: {}", member.path),
                    detail: Some(json!({
                        "path": member.path,
                        "expected": member.bytes_hash,
                        "actual": actual_hash
                    })),
                });
                all_hashes_valid = false;
            }
        }

        checks.member_hashes = all_hashes_valid;
        Ok(())
    }

    /// Verify pack_id computation
    fn verify_pack_id(
        &self,
        manifest: &Manifest,
        checks: &mut IntegrityChecks,
        invalid_findings: &mut Vec<InvalidFinding>,
    ) -> Result<(), VerificationError> {
        // Get manifest with pack_id="" for recomputation
        let hash_manifest = manifest.for_hash_computation();

        // Serialize to canonical JSON
        let canonical_bytes = to_canonical_json(&hash_manifest).map_err(|e| VerificationError::Io {
            path: Some(self.pack_dir.join("manifest.json")),
            operation: "canonical_serialization".to_string(),
            error: e.to_string(),
        })?;

        // Compute SHA256 hash
        let computed_pack_id = crate::copy::hasher::hash_bytes(&canonical_bytes);

        if computed_pack_id != manifest.pack_id {
            invalid_findings.push(InvalidFinding {
                code: "PACK_ID_MISMATCH".to_string(),
                message: "Pack ID does not match recomputed hash".to_string(),
                detail: Some(json!({
                    "expected": manifest.pack_id,
                    "actual": computed_pack_id
                })),
            });
            checks.pack_id = false;
        } else {
            checks.pack_id = true;
        }

        Ok(())
    }

    /// Check if path is safe and relative
    fn is_safe_relative_path(&self, path: &str) -> bool {
        // Must not be empty
        if path.is_empty() {
            return false;
        }

        // Must not be absolute
        if path.starts_with('/') {
            return false;
        }

        // Check each component
        for component in path.split('/') {
            match component {
                // Empty components (double slashes) not allowed
                "" => return false,
                // Parent directory escapes not allowed
                ".." => return false,
                // Current directory components not allowed
                "." => return false,
                // Regular components are fine
                _ => continue,
            }
        }

        true
    }
}

/// Verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    /// Overall outcome
    pub outcome: VerifyOutcome,

    /// Pack ID that was verified
    pub pack_id: String,

    /// Individual check results
    pub checks: IntegrityChecks,

    /// Invalid findings (empty for OK outcome)
    pub invalid_findings: Vec<InvalidFinding>,
}

impl VerifyResult {
    /// Get exit code for this result
    pub fn exit_code(&self) -> u8 {
        match self.outcome {
            VerifyOutcome::Ok => 0,
            VerifyOutcome::Invalid => 1,
        }
    }

    /// Convert to JSON output
    pub fn to_json(&self) -> serde_json::Result<String> {
        let output = VerifyJsonOutput {
            version: "pack.verify.v0".to_string(),
            outcome: self.outcome.clone(),
            pack_id: self.pack_id.clone(),
            checks: self.checks.clone(),
            invalid: self.invalid_findings.clone(),
            refusal: None,
        };
        serde_json::to_string_pretty(&output)
    }

    /// Generate human-readable output
    pub fn to_human_output(&self) -> String {
        let mut output = String::new();

        match self.outcome {
            VerifyOutcome::Ok => {
                output.push_str(&format!("✓ Pack verification successful\n"));
                output.push_str(&format!("Pack ID: {}\n", self.pack_id));
                output.push_str(&format!("All integrity checks passed\n"));
            }
            VerifyOutcome::Invalid => {
                output.push_str(&format!("✗ Pack verification failed\n"));
                output.push_str(&format!("Pack ID: {}\n", self.pack_id));
                output.push_str(&format!("Found {} integrity issue(s):\n\n", self.invalid_findings.len()));

                for (i, finding) in self.invalid_findings.iter().enumerate() {
                    output.push_str(&format!("{}. {}: {}\n", i + 1, finding.code, finding.message));
                }
            }
        }

        output
    }
}

/// Verification outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum VerifyOutcome {
    Ok,
    Invalid,
}

/// Individual integrity checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityChecks {
    pub manifest_parse: bool,
    pub member_count: bool,
    pub member_paths: bool,
    pub extra_members: bool,
    pub member_files: bool,
    pub member_hashes: bool,
    pub pack_id: bool,
    pub schema_validation: String, // "pass" | "fail" | "skipped"
}

impl IntegrityChecks {
    fn new() -> Self {
        Self {
            manifest_parse: true, // If we get here, manifest parsed successfully
            member_count: false,
            member_paths: false,
            extra_members: false,
            member_files: false,
            member_hashes: false,
            pack_id: false,
            schema_validation: "skipped".to_string(), // Schema validation tracked separately
        }
    }
}

/// Invalid finding details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidFinding {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// JSON output format for verify command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyJsonOutput {
    pub version: String,
    pub outcome: VerifyOutcome,
    pub pack_id: String,
    pub checks: IntegrityChecks,
    pub invalid: Vec<InvalidFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<serde_json::Value>,
}

/// Verification errors
#[derive(Debug)]
pub enum VerificationError {
    /// IO operation failed
    Io {
        path: Option<PathBuf>,
        operation: String,
        error: String,
    },
    /// Bad pack (missing/invalid manifest)
    BadPack {
        pack_dir: PathBuf,
        issue: String,
    },
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationError::Io { path, operation, error } => {
                let path_str = path.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "unknown path".to_string());
                write!(f, "IO operation '{}' failed on {}: {}", operation, path_str, error)
            }
            VerificationError::BadPack { pack_dir, issue } => {
                write!(f, "Bad pack at {}: {}", pack_dir.display(), issue)
            }
        }
    }
}

impl std::error::Error for VerificationError {}

impl VerificationError {
    /// Convert to refusal code and detail
    pub fn to_refusal(&self) -> (RefusalCode, crate::refusal::RefusalDetail) {
        match self {
            VerificationError::Io { path, operation, error } => {
                RefusalCode::io_error(
                    path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    operation.clone(),
                    error.clone(),
                )
            }
            VerificationError::BadPack { pack_dir, issue } => {
                RefusalCode::bad_pack(
                    pack_dir.to_string_lossy().to_string(),
                    issue.clone(),
                )
            }
        }
    }
}