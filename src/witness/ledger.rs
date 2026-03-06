use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::record::{canonical_json, WitnessRecord};

/// Determine the witness ledger path.
///
/// Priority:
/// 1. `EPISTEMIC_WITNESS` env var
/// 2. `~/.epistemic/witness.jsonl`
pub fn witness_ledger_path() -> PathBuf {
    witness_ledger_path_from_env(|key| std::env::var(key).ok())
}

fn witness_ledger_path_from_env<F>(get_env: F) -> PathBuf
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(path) = get_env("EPISTEMIC_WITNESS") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    let home = home_from_env(&get_env).unwrap_or_else(|| PathBuf::from("."));
    home.join(".epistemic").join("witness.jsonl")
}

fn home_from_env<F>(get_env: &F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    #[cfg(unix)]
    {
        get_env("HOME")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        get_env("USERPROFILE")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Append a witness record to the ledger.
///
/// Returns `Ok(())` on success, `Err(message)` on failure.
/// Witness failures should be warned but must not change domain exit semantics.
pub fn append_witness(record: &WitnessRecord) -> Result<(), String> {
    let path = witness_ledger_path();

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create witness directory: {e}"))?;
    }

    let mut record = record.clone();
    if record.prev.is_none() {
        record.prev = last_record_id(&path);
    }
    record.compute_id();
    let line = canonical_json(&record);

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Cannot open witness ledger: {e}"))?;

    writeln!(file, "{line}").map_err(|e| format!("Cannot write witness record: {e}"))?;

    Ok(())
}

fn last_record_id(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut last_non_empty = None;
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            last_non_empty = Some(trimmed.to_owned());
        }
    }

    let last = last_non_empty?;
    let value: serde_json::Value = serde_json::from_str(&last).ok()?;
    value.get("id")?.as_str().map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn append_creates_file_and_writes_record() {
        let tmp = TempDir::new().unwrap();
        let ledger_path = tmp.path().join("witness.jsonl");

        // Override env for test
        std::env::set_var("EPISTEMIC_WITNESS", ledger_path.display().to_string());

        let record = WitnessRecord::new(
            "seal",
            vec![crate::witness::WitnessInput {
                path: "artifact.json".to_string(),
                hash: Some("sha256:abc".to_string()),
                bytes: Some(7),
            }],
            "PACK_CREATED",
            0,
            serde_json::Map::new(),
            b"PACK_CREATED sha256:abc\n/tmp/out\n",
            Some("sha256:abc".to_string()),
        );
        append_witness(&record).unwrap();

        let content = fs::read_to_string(&ledger_path).unwrap();
        let parsed: WitnessRecord = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed.tool, "pack");
        assert_eq!(parsed.command.as_deref(), Some("seal"));
        assert_eq!(parsed.outcome, "PACK_CREATED");
        assert!(parsed.id.starts_with("blake3:"));
        assert_eq!(parsed.prev, None);

        std::env::remove_var("EPISTEMIC_WITNESS");
    }

    #[test]
    fn append_is_additive() {
        let tmp = TempDir::new().unwrap();
        let ledger_path = tmp.path().join("witness.jsonl");
        std::env::set_var("EPISTEMIC_WITNESS", ledger_path.display().to_string());

        let r1 = WitnessRecord::new(
            "seal",
            Vec::new(),
            "PACK_CREATED",
            0,
            serde_json::Map::new(),
            b"PACK_CREATED sha256:abc\n/tmp/out\n",
            None,
        );
        let r2 = WitnessRecord::new(
            "verify",
            Vec::new(),
            "OK",
            0,
            serde_json::Map::new(),
            b"pack verify: OK\n",
            Some("sha256:xyz".to_string()),
        );
        append_witness(&r1).unwrap();
        append_witness(&r2).unwrap();

        let content = fs::read_to_string(&ledger_path).unwrap();
        let lines: Vec<&str> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert_eq!(lines.len(), 2);
        let first: WitnessRecord = serde_json::from_str(lines[0]).unwrap();
        let second: WitnessRecord = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second.prev.as_deref(), Some(first.id.as_str()));

        std::env::remove_var("EPISTEMIC_WITNESS");
    }

    #[test]
    fn witness_record_has_correct_fields() {
        let record = WitnessRecord::new(
            "seal",
            Vec::new(),
            "PACK_CREATED",
            0,
            serde_json::Map::new(),
            b"PACK_CREATED sha256:abc\n/tmp/out\n",
            Some("sha256:abc".to_string()),
        );
        assert_eq!(record.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(record.tool, "pack");
        assert!(!record.ts.is_empty());
        assert!(record.binary_hash.starts_with("blake3:"));
        assert!(record.output_hash.starts_with("blake3:"));
    }

    #[test]
    fn empty_epistemic_witness_falls_back_to_home() {
        let path = witness_ledger_path_from_env(|key| match key {
            "EPISTEMIC_WITNESS" => Some(String::new()),
            "HOME" => Some("/tmp/home".to_string()),
            _ => None,
        });

        assert_eq!(path, PathBuf::from("/tmp/home/.epistemic/witness.jsonl"));
    }

    #[test]
    fn empty_home_falls_back_to_repo_epistemic_dir() {
        let path = witness_ledger_path_from_env(|key| match key {
            "EPISTEMIC_WITNESS" => None,
            "HOME" => Some(String::new()),
            _ => None,
        });

        assert_eq!(path, PathBuf::from(".").join(".epistemic/witness.jsonl"));
    }
}
