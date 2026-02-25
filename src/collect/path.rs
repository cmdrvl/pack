//! Path normalization and safety checks

use std::path::{Path, Component};

/// Normalize a path to be relative, POSIX-style, and safe for pack members
pub fn normalize_member_path<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let path = path.as_ref();

    // Convert to POSIX-style separators and normalize
    let normalized = normalize_path_components(path)?;

    // Ensure it's safe (relative, no .. escapes)
    if !is_safe_relative_path(&normalized) {
        anyhow::bail!("Unsafe path: contains absolute or escape components");
    }

    Ok(normalized)
}

/// Check if a path is safe for pack members (relative, no .. escapes)
pub fn is_safe_relative_path(path: &str) -> bool {
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
            // Current directory components are okay but should be normalized out
            "." => return false,
            // Regular components are fine
            _ => continue,
        }
    }

    true
}

/// Normalize path components to relative POSIX style
fn normalize_path_components<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let path = path.as_ref();
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(name) => {
                // Convert OsStr to UTF-8 string
                let name_str = name.to_str()
                    .ok_or_else(|| anyhow::anyhow!("Path contains invalid UTF-8"))?;
                components.push(name_str.to_string());
            }
            Component::CurDir => {
                // Skip current directory components
                continue;
            }
            Component::ParentDir => {
                // Handle parent directory traversal
                if components.is_empty() {
                    anyhow::bail!("Path escapes above root");
                }
                components.pop();
            }
            Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("Absolute paths not allowed");
            }
        }
    }

    if components.is_empty() {
        anyhow::bail!("Path resolves to empty");
    }

    Ok(components.join("/"))
}

/// Extract filename for use as default member path
pub fn extract_filename<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let path = path.as_ref();

    let filename = path.file_name()
        .ok_or_else(|| anyhow::anyhow!("Path has no filename"))?
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Filename contains invalid UTF-8"))?;

    Ok(filename.to_string())
}

/// Create member path from directory basename and relative path
pub fn create_member_path(dir_basename: &str, relative_path: &str) -> String {
    format!("{}/{}", dir_basename, relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_normalize_member_path() {
        // Simple relative path
        assert_eq!(
            normalize_member_path("test.txt").unwrap(),
            "test.txt"
        );

        // Nested path
        assert_eq!(
            normalize_member_path("dir/test.txt").unwrap(),
            "dir/test.txt"
        );

        // Path with current directory
        assert_eq!(
            normalize_member_path("./dir/test.txt").unwrap(),
            "dir/test.txt"
        );

        // Path with parent directory (but stays within bounds)
        assert_eq!(
            normalize_member_path("dir/../test.txt").unwrap(),
            "test.txt"
        );
    }

    #[test]
    fn test_normalize_member_path_failures() {
        // Absolute path
        assert!(normalize_member_path("/absolute/path").is_err());

        // Path that escapes above root
        assert!(normalize_member_path("../escape").is_err());

        // Path that resolves to empty
        assert!(normalize_member_path("./").is_err());
        assert!(normalize_member_path(".").is_err());
    }

    #[test]
    fn test_is_safe_relative_path() {
        // Safe paths
        assert!(is_safe_relative_path("test.txt"));
        assert!(is_safe_relative_path("dir/test.txt"));
        assert!(is_safe_relative_path("deep/nested/path/file.json"));

        // Unsafe paths
        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("/absolute"));
        assert!(!is_safe_relative_path("../escape"));
        assert!(!is_safe_relative_path("dir/../escape"));
        assert!(!is_safe_relative_path("."));
        assert!(!is_safe_relative_path("./current"));
        assert!(!is_safe_relative_path("dir//double"));
    }

    #[test]
    fn test_extract_filename() {
        assert_eq!(extract_filename("test.txt").unwrap(), "test.txt");
        assert_eq!(extract_filename("dir/test.txt").unwrap(), "test.txt");
        assert_eq!(extract_filename("/absolute/path/file.json").unwrap(), "file.json");
    }

    #[test]
    fn test_create_member_path() {
        assert_eq!(
            create_member_path("reports", "nov.json"),
            "reports/nov.json"
        );
        assert_eq!(
            create_member_path("data", "nested/file.csv"),
            "data/nested/file.csv"
        );
    }

    #[test]
    fn test_cross_platform_normalization() {
        // Test with PathBuf to simulate cross-platform behavior
        #[cfg(windows)]
        {
            let win_path = PathBuf::from(r"dir\subdir\file.txt");
            assert_eq!(
                normalize_member_path(&win_path).unwrap(),
                "dir/subdir/file.txt"
            );
        }

        #[cfg(unix)]
        {
            let unix_path = PathBuf::from("dir/subdir/file.txt");
            assert_eq!(
                normalize_member_path(&unix_path).unwrap(),
                "dir/subdir/file.txt"
            );
        }
    }
}