use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use super::record::WitnessRecord;

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

    let line =
        serde_json::to_string(record).map_err(|e| format!("Cannot serialize witness: {e}"))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Cannot open witness ledger: {e}"))?;

    writeln!(file, "{line}").map_err(|e| format!("Cannot write witness record: {e}"))?;

    Ok(())
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

        let record = WitnessRecord::new("seal", "PACK_CREATED", Some("sha256:abc".to_string()));
        append_witness(&record).unwrap();

        let content = fs::read_to_string(&ledger_path).unwrap();
        let parsed: WitnessRecord = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed.tool, "pack");
        assert_eq!(parsed.command, "seal");
        assert_eq!(parsed.outcome, "PACK_CREATED");

        std::env::remove_var("EPISTEMIC_WITNESS");
    }

    #[test]
    fn append_is_additive() {
        let tmp = TempDir::new().unwrap();
        let ledger_path = tmp.path().join("witness.jsonl");
        std::env::set_var("EPISTEMIC_WITNESS", ledger_path.display().to_string());

        let r1 = WitnessRecord::new("seal", "PACK_CREATED", None);
        let r2 = WitnessRecord::new("verify", "OK", Some("sha256:xyz".to_string()));
        append_witness(&r1).unwrap();
        append_witness(&r2).unwrap();

        let content = fs::read_to_string(&ledger_path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        std::env::remove_var("EPISTEMIC_WITNESS");
    }

    #[test]
    fn witness_record_has_correct_fields() {
        let record = WitnessRecord::new("seal", "PACK_CREATED", Some("sha256:abc".to_string()));
        assert_eq!(record.version, "witness.v0");
        assert_eq!(record.tool, "pack");
        assert!(!record.timestamp.is_empty());
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
