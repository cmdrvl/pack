use std::fs;
use std::io::BufRead;

use super::ledger::witness_ledger_path;
use super::record::WitnessRecord;

/// Read all witness records from the ledger, filtered to pack tool only.
fn read_ledger() -> Vec<WitnessRecord> {
    let path = witness_ledger_path();
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .as_bytes()
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            let record: WitnessRecord = serde_json::from_str(&line).ok()?;
            if record.tool == "pack" {
                Some(record)
            } else {
                None
            }
        })
        .collect()
}

/// Execute `pack witness query` — return all pack witness records.
pub fn execute_query(json_output: bool) -> String {
    let records = read_ledger();
    if records.is_empty() {
        return if json_output {
            "[]".to_string()
        } else {
            "No witness records found.".to_string()
        };
    }

    if json_output {
        serde_json::to_string_pretty(&records).unwrap_or_else(|_| "[]".to_string())
    } else {
        records
            .iter()
            .map(format_record_human)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Execute `pack witness last` — return the most recent pack witness record.
pub fn execute_last(json_output: bool) -> String {
    let records = read_ledger();
    match records.last() {
        Some(record) => {
            if json_output {
                serde_json::to_string_pretty(record).unwrap_or_else(|_| "null".to_string())
            } else {
                format_record_human(record)
            }
        }
        None => {
            if json_output {
                "null".to_string()
            } else {
                "No witness records found.".to_string()
            }
        }
    }
}

/// Execute `pack witness count` — return count of pack witness records.
pub fn execute_count(json_output: bool) -> String {
    let records = read_ledger();
    if json_output {
        serde_json::json!({"count": records.len()}).to_string()
    } else {
        format!("{} witness record(s)", records.len())
    }
}

fn format_record_human(r: &WitnessRecord) -> String {
    let pack_id = r.pack_id.as_deref().unwrap_or("-");
    format!("{} {} {} {}", r.timestamp, r.command, r.outcome, pack_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::witness::append_witness;
    use tempfile::TempDir;

    fn setup_ledger() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let ledger_path = tmp.path().join("witness.jsonl");
        std::env::set_var("EPISTEMIC_WITNESS", ledger_path.display().to_string());
        tmp
    }

    fn teardown() {
        std::env::remove_var("EPISTEMIC_WITNESS");
    }

    #[test]
    fn query_empty_ledger() {
        let _tmp = setup_ledger();
        let result = execute_query(false);
        assert_eq!(result, "No witness records found.");
        let json_result = execute_query(true);
        assert_eq!(json_result, "[]");
        teardown();
    }

    #[test]
    fn query_returns_records() {
        let _tmp = setup_ledger();
        let r = WitnessRecord::new("seal", "PACK_CREATED", Some("sha256:abc".to_string()));
        append_witness(&r).unwrap();

        let result = execute_query(false);
        assert!(result.contains("seal"));
        assert!(result.contains("PACK_CREATED"));

        let json_result = execute_query(true);
        let parsed: Vec<WitnessRecord> = serde_json::from_str(&json_result).unwrap();
        assert_eq!(parsed.len(), 1);
        teardown();
    }

    #[test]
    fn last_returns_most_recent() {
        let _tmp = setup_ledger();
        let r1 = WitnessRecord::new("seal", "PACK_CREATED", None);
        let r2 = WitnessRecord::new("verify", "OK", None);
        append_witness(&r1).unwrap();
        append_witness(&r2).unwrap();

        let result = execute_last(false);
        assert!(result.contains("verify"));
        assert!(result.contains("OK"));

        let json_result = execute_last(true);
        let parsed: WitnessRecord = serde_json::from_str(&json_result).unwrap();
        assert_eq!(parsed.command, "verify");
        teardown();
    }

    #[test]
    fn last_empty_ledger() {
        let _tmp = setup_ledger();
        let result = execute_last(false);
        assert_eq!(result, "No witness records found.");
        let json_result = execute_last(true);
        assert_eq!(json_result, "null");
        teardown();
    }

    #[test]
    fn count_returns_correct_number() {
        let _tmp = setup_ledger();
        let r1 = WitnessRecord::new("seal", "PACK_CREATED", None);
        let r2 = WitnessRecord::new("verify", "OK", None);
        append_witness(&r1).unwrap();
        append_witness(&r2).unwrap();

        let result = execute_count(false);
        assert_eq!(result, "2 witness record(s)");

        let json_result = execute_count(true);
        let parsed: serde_json::Value = serde_json::from_str(&json_result).unwrap();
        assert_eq!(parsed["count"], 2);
        teardown();
    }

    #[test]
    fn count_empty_ledger() {
        let _tmp = setup_ledger();
        let result = execute_count(false);
        assert_eq!(result, "0 witness record(s)");
        teardown();
    }
}
