//! SHA256 hashing utilities for member bytes

use sha2::{Sha256, Digest};
use std::fs::File;
use std::io::{self, Read, BufReader};
use std::path::Path;

/// Compute SHA256 hash of bytes and return as hex string with "sha256:" prefix
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    format!("sha256:{:x}", result)
}

/// Compute SHA256 hash of a file and return as hex string with "sha256:" prefix
pub fn compute_sha256_hex<P: AsRef<Path>>(file_path: P) -> io::Result<String> {
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    let mut buffer = [0; 8192]; // 8KB buffer for efficient reading
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("sha256:{:x}", result))
}

/// Compute SHA256 hash from a reader and return as hex string with "sha256:" prefix
pub fn hash_from_reader<R: Read>(mut reader: R) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("sha256:{:x}", result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_hash_bytes() {
        // Test known hash
        let data = b"hello world";
        let hash = hash_bytes(data);

        // Expected SHA256 of "hello world"
        assert_eq!(hash, "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_hash_bytes_empty() {
        let data = b"";
        let hash = hash_bytes(data);

        // Expected SHA256 of empty string
        assert_eq!(hash, "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_compute_sha256_hex() -> anyhow::Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        write!(temp_file, "test content")?;

        let hash = compute_sha256_hex(temp_file.path())?;

        // Verify it starts with sha256:
        assert!(hash.starts_with("sha256:"));

        // Verify it matches direct byte hash
        let expected = hash_bytes(b"test content");
        assert_eq!(hash, expected);

        Ok(())
    }

    #[test]
    fn test_hash_from_reader() -> anyhow::Result<()> {
        let data = b"reader test data";
        let cursor = Cursor::new(data);

        let hash = hash_from_reader(cursor)?;
        let expected = hash_bytes(data);

        assert_eq!(hash, expected);

        Ok(())
    }

    #[test]
    fn test_deterministic_hashing() {
        let data = b"deterministic test";

        let hash1 = hash_bytes(data);
        let hash2 = hash_bytes(data);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_data_different_hash() {
        let data1 = b"data one";
        let data2 = b"data two";

        let hash1 = hash_bytes(data1);
        let hash2 = hash_bytes(data2);

        assert_ne!(hash1, hash2);
    }
}