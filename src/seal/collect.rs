use std::fs;
use std::path::{Path, PathBuf};

use crate::refusal::{RefusalCode, RefusalEnvelope};

/// A candidate member resolved from input artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberCandidate {
    /// The source path on disk.
    pub source: PathBuf,
    /// The normalized relative member path within the pack.
    pub member_path: String,
}

/// Shorthand for creating a boxed refusal.
fn refusal(
    code: RefusalCode,
    message: Option<String>,
    detail: Option<serde_json::Value>,
) -> Box<RefusalEnvelope> {
    Box::new(RefusalEnvelope::new(code, message, detail))
}

/// Collect artifacts from input paths into a sorted list of member candidates.
///
/// - File arguments become a single member using the file's basename.
/// - Directory arguments are recursively walked; members use `<dir_basename>/<relative_path>`.
/// - Only regular files are admissible; symlinks/sockets/devices/FIFOs produce an error.
/// - Results are sorted by bytewise ascending member path.
pub fn collect_artifacts(inputs: &[PathBuf]) -> Result<Vec<MemberCandidate>, Box<RefusalEnvelope>> {
    if inputs.is_empty() {
        return Err(refusal(RefusalCode::Empty, None, None));
    }

    let mut candidates = Vec::new();

    for input in inputs {
        let meta = fs::symlink_metadata(input).map_err(|e| {
            refusal(
                RefusalCode::Io,
                Some(format!("Cannot read input: {}: {e}", input.display())),
                None,
            )
        })?;

        if meta.is_symlink() {
            return Err(refusal(
                RefusalCode::Io,
                Some(format!("Non-regular input (symlink): {}", input.display())),
                None,
            ));
        }

        if meta.is_file() {
            let member_path = input
                .file_name()
                .ok_or_else(|| {
                    refusal(
                        RefusalCode::Io,
                        Some(format!("Cannot determine filename: {}", input.display())),
                        None,
                    )
                })?
                .to_string_lossy()
                .to_string();

            candidates.push(MemberCandidate {
                source: input.clone(),
                member_path,
            });
        } else if meta.is_dir() {
            collect_dir(input, input, &mut candidates)?;
        } else {
            return Err(refusal(
                RefusalCode::Io,
                Some(format!("Non-regular input: {}", input.display())),
                None,
            ));
        }
    }

    // Deterministic: bytewise ascending path order.
    candidates.sort_by(|a, b| a.member_path.cmp(&b.member_path));

    Ok(candidates)
}

/// Recursively collect regular files from a directory.
fn collect_dir(
    root: &Path,
    dir: &Path,
    candidates: &mut Vec<MemberCandidate>,
) -> Result<(), Box<RefusalEnvelope>> {
    let dir_basename = root
        .file_name()
        .ok_or_else(|| {
            refusal(
                RefusalCode::Io,
                Some(format!(
                    "Cannot determine directory name: {}",
                    root.display()
                )),
                None,
            )
        })?
        .to_string_lossy();

    // Collect and sort entries for deterministic traversal.
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
        .map_err(|e| {
            refusal(
                RefusalCode::Io,
                Some(format!("Cannot read directory: {}: {e}", dir.display())),
                None,
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            refusal(
                RefusalCode::Io,
                Some(format!(
                    "Error reading directory entry: {}: {e}",
                    dir.display()
                )),
                None,
            )
        })?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let meta = entry.metadata().map_err(|e| {
            refusal(
                RefusalCode::Io,
                Some(format!("Cannot stat: {}: {e}", entry.path().display())),
                None,
            )
        })?;

        // Check symlink via symlink_metadata
        let sym_meta = fs::symlink_metadata(entry.path()).map_err(|e| {
            refusal(
                RefusalCode::Io,
                Some(format!("Cannot stat: {}: {e}", entry.path().display())),
                None,
            )
        })?;
        if sym_meta.is_symlink() {
            return Err(refusal(
                RefusalCode::Io,
                Some(format!(
                    "Non-regular input (symlink): {}",
                    entry.path().display()
                )),
                None,
            ));
        }

        if meta.is_dir() {
            collect_dir(root, &entry.path(), candidates)?;
        } else if meta.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|e| {
                    refusal(
                        RefusalCode::Io,
                        Some(format!("Path prefix error: {e}")),
                        None,
                    )
                })?
                .to_string_lossy()
                .to_string();

            // Normalize to POSIX-style path: <dir_basename>/<relative>
            let member_path = normalize_member_path(&format!("{dir_basename}/{relative}"));

            candidates.push(MemberCandidate {
                source: entry.path(),
                member_path,
            });
        } else {
            return Err(refusal(
                RefusalCode::Io,
                Some(format!("Non-regular input: {}", entry.path().display())),
                None,
            ));
        }
    }

    Ok(())
}

/// Normalize a member path to safe relative POSIX-style:
/// - Use `/` separators
/// - No absolute paths
/// - No `..` segments
fn normalize_member_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Validate that a member path is safe (no absolute, no `..`).
pub fn is_safe_member_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.starts_with('/') {
        return false;
    }
    for segment in path.split('/') {
        if segment == ".." {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn empty_inputs_returns_e_empty() {
        let result = collect_artifacts(&[]);
        let err = result.unwrap_err();
        assert_eq!(err.refusal.code, "E_EMPTY");
    }

    #[test]
    fn single_file_uses_basename() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("report.json");
        fs::write(&file, "{}").unwrap();

        let candidates = collect_artifacts(&[file]).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].member_path, "report.json");
    }

    #[test]
    fn directory_recurse_uses_dir_prefix() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("evidence");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("a.json"), "{}").unwrap();
        fs::write(dir.join("b.json"), "{}").unwrap();

        let candidates = collect_artifacts(&[dir]).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].member_path, "evidence/a.json");
        assert_eq!(candidates[1].member_path, "evidence/b.json");
    }

    #[test]
    fn nested_directory_collects_recursively() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("reg");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.join("top.json"), "{}").unwrap();
        fs::write(sub.join("deep.json"), "{}").unwrap();

        let candidates = collect_artifacts(&[dir]).unwrap();
        assert_eq!(candidates.len(), 2);
        // Sorted bytewise
        assert_eq!(candidates[0].member_path, "reg/sub/deep.json");
        assert_eq!(candidates[1].member_path, "reg/top.json");
    }

    #[test]
    fn results_are_sorted_bytewise() {
        let tmp = TempDir::new().unwrap();
        let z = tmp.path().join("z.json");
        let a = tmp.path().join("a.json");
        let m = tmp.path().join("m.json");
        fs::write(&z, "{}").unwrap();
        fs::write(&a, "{}").unwrap();
        fs::write(&m, "{}").unwrap();

        let candidates = collect_artifacts(&[z, a, m]).unwrap();
        let paths: Vec<&str> = candidates.iter().map(|c| c.member_path.as_str()).collect();
        assert_eq!(paths, vec!["a.json", "m.json", "z.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_refuses_with_e_io() {
        use std::os::unix::fs as unix_fs;
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().join("real.json");
        let link = tmp.path().join("link.json");
        fs::write(&real, "{}").unwrap();
        unix_fs::symlink(&real, &link).unwrap();

        let result = collect_artifacts(&[link]);
        let err = result.unwrap_err();
        assert_eq!(err.refusal.code, "E_IO");
        assert!(err.refusal.message.contains("symlink"));
    }

    #[test]
    fn nonexistent_input_refuses_with_e_io() {
        let result = collect_artifacts(&[PathBuf::from("/nonexistent/file.json")]);
        let err = result.unwrap_err();
        assert_eq!(err.refusal.code, "E_IO");
    }

    #[test]
    fn safe_member_path_checks() {
        assert!(is_safe_member_path("a.json"));
        assert!(is_safe_member_path("dir/a.json"));
        assert!(is_safe_member_path("dir/sub/a.json"));
        assert!(!is_safe_member_path(""));
        assert!(!is_safe_member_path("/absolute/path"));
        assert!(!is_safe_member_path("../escape"));
        assert!(!is_safe_member_path("dir/../escape"));
    }
}
