use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::paths::{witness_path, witness_path_for_append, witness_path_for_query};

use super::record::{canonical_json, WitnessRecord};

/// Determine the witness ledger path.
///
/// Priority:
/// 1. `EPISTEMIC_WITNESS` env var
/// 2. `~/.cmdrvl/state/witness/witness.jsonl`
pub fn witness_ledger_path() -> PathBuf {
    witness_path()
}

pub(crate) fn witness_ledger_path_for_query() -> Result<PathBuf, String> {
    witness_path_for_query()
}

/// Append a witness record to the ledger.
///
/// Returns `Ok(())` on success, `Err(message)` on failure.
/// Witness failures should be warned but must not change domain exit semantics.
pub fn append_witness(record: &WitnessRecord) -> Result<(), String> {
    let path = witness_path_for_append()?;

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create witness directory: {e}"))?;
    }

    let mut record = record.clone();
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
        assert_ne!(second.id, first.id);
        assert_eq!(second.outcome, "OK");

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
}
