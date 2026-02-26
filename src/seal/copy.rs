use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

use super::collect::MemberCandidate;
use crate::refusal::{RefusalCode, RefusalEnvelope};

/// Result of copying a single member into the pack output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedMember {
    /// The member path within the pack directory.
    pub member_path: String,
    /// `sha256:<hex>` hash of the copied bytes.
    pub bytes_hash: String,
    /// Number of bytes copied.
    pub size: u64,
}

/// Copy members into the staging directory and compute their SHA256 hashes.
///
/// For each candidate:
/// - Creates parent directories as needed under `staging_dir`.
/// - Copies bytes exactly from source to `staging_dir/<member_path>`.
/// - Computes `sha256:<hex>` hash from the copied bytes.
pub fn copy_and_hash(
    candidates: &[MemberCandidate],
    staging_dir: &Path,
) -> Result<Vec<CopiedMember>, Box<RefusalEnvelope>> {
    let mut results = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let dest = staging_dir.join(&candidate.member_path);

        // Create parent directories if needed.
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| io_refusal(&candidate.member_path, e))?;
        }

        // Copy and hash in one pass.
        let (bytes_hash, size) =
            copy_and_hash_file(&candidate.source, &dest, &candidate.member_path)?;

        results.push(CopiedMember {
            member_path: candidate.member_path.clone(),
            bytes_hash,
            size,
        });
    }

    Ok(results)
}

/// Copy a single file while computing its SHA256 hash.
fn copy_and_hash_file(
    source: &Path,
    dest: &Path,
    member_path: &str,
) -> Result<(String, u64), Box<RefusalEnvelope>> {
    let mut reader =
        fs::File::open(source).map_err(|e| io_refusal_detail(member_path, "read source", e))?;
    let mut writer =
        fs::File::create(dest).map_err(|e| io_refusal_detail(member_path, "write dest", e))?;

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    let mut total: u64 = 0;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| io_refusal_detail(member_path, "read", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer
            .write_all(&buf[..n])
            .map_err(|e| io_refusal_detail(member_path, "write", e))?;
        total += n as u64;
    }

    let hash = hex::encode(hasher.finalize());
    Ok((format!("sha256:{hash}"), total))
}

fn io_refusal(member_path: &str, err: io::Error) -> Box<RefusalEnvelope> {
    Box::new(RefusalEnvelope::new(
        RefusalCode::Io,
        Some(format!("IO error for member '{member_path}': {err}")),
        None,
    ))
}

fn io_refusal_detail(member_path: &str, op: &str, err: io::Error) -> Box<RefusalEnvelope> {
    Box::new(RefusalEnvelope::new(
        RefusalCode::Io,
        Some(format!("IO error ({op}) for member '{member_path}': {err}")),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_candidate(tmp: &TempDir, name: &str, content: &[u8]) -> MemberCandidate {
        let path = tmp.path().join(name);
        fs::write(&path, content).unwrap();
        MemberCandidate {
            source: path,
            member_path: name.to_string(),
        }
    }

    #[test]
    fn copy_preserves_bytes() {
        let src_tmp = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let content = b"hello world";
        let candidate = make_candidate(&src_tmp, "test.json", content);

        let results = copy_and_hash(&[candidate], staging.path()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].size, content.len() as u64);

        // Verify bytes are identical.
        let copied = fs::read(staging.path().join("test.json")).unwrap();
        assert_eq!(copied, content);
    }

    #[test]
    fn hash_is_deterministic() {
        let src_tmp = TempDir::new().unwrap();
        let staging1 = TempDir::new().unwrap();
        let staging2 = TempDir::new().unwrap();
        let content = b"deterministic content";
        let c1 = make_candidate(&src_tmp, "a.json", content);
        let c2 = MemberCandidate {
            source: c1.source.clone(),
            member_path: "a.json".to_string(),
        };

        let r1 = copy_and_hash(&[c1], staging1.path()).unwrap();
        let r2 = copy_and_hash(&[c2], staging2.path()).unwrap();
        assert_eq!(r1[0].bytes_hash, r2[0].bytes_hash);
    }

    #[test]
    fn hash_format_is_sha256_hex() {
        let src_tmp = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let candidate = make_candidate(&src_tmp, "f.json", b"{}");

        let results = copy_and_hash(&[candidate], staging.path()).unwrap();
        assert!(results[0].bytes_hash.starts_with("sha256:"));
        assert_eq!(results[0].bytes_hash.len(), 7 + 64);
    }

    #[test]
    fn creates_parent_dirs_for_nested_members() {
        let src_tmp = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let path = src_tmp.path().join("deep.json");
        fs::write(&path, "{}").unwrap();
        let candidate = MemberCandidate {
            source: path,
            member_path: "dir/sub/deep.json".to_string(),
        };

        let results = copy_and_hash(&[candidate], staging.path()).unwrap();
        assert_eq!(results.len(), 1);
        assert!(staging.path().join("dir/sub/deep.json").exists());
    }

    #[test]
    fn missing_source_returns_e_io() {
        let staging = TempDir::new().unwrap();
        let candidate = MemberCandidate {
            source: PathBuf::from("/nonexistent/source.json"),
            member_path: "source.json".to_string(),
        };

        let err = copy_and_hash(&[candidate], staging.path()).unwrap_err();
        assert_eq!(err.refusal.code, "E_IO");
    }

    #[test]
    fn empty_file_hashes_correctly() {
        let src_tmp = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let candidate = make_candidate(&src_tmp, "empty.json", b"");

        let results = copy_and_hash(&[candidate], staging.path()).unwrap();
        assert_eq!(results[0].size, 0);
        assert!(results[0].bytes_hash.starts_with("sha256:"));
    }
}
