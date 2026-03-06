use std::fs;
use std::io::{BufRead, BufReader};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::cli::WitnessFilters;

use super::ledger::witness_ledger_path;
use super::record::WitnessRecord;

fn read_ledger() -> Vec<WitnessRecord> {
    let path = witness_ledger_path();
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<WitnessRecord>(&line).ok())
        .collect()
}

/// Execute `pack witness query` — return matching witness records.
pub fn execute_query(filters: &WitnessFilters, json_output: bool) -> String {
    let records = read_ledger();
    let records = filter_records(&records, filters, true);
    if records.is_empty() {
        return if json_output {
            "[]".to_string()
        } else if filters_active(filters) {
            "No matching witness records.".to_string()
        } else {
            "No witness records found.".to_string()
        };
    }

    if json_output {
        serde_json::to_string_pretty(&records).unwrap_or_else(|_| "[]".to_string())
    } else {
        records
            .iter()
            .map(|record| format_record_human(record))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Execute `pack witness last` — return the most recent pack witness record.
pub fn execute_last(json_output: bool) -> String {
    let records = read_ledger();
    let filters = WitnessFilters::default();
    let record = filter_records(&records, &filters, true).into_iter().last();
    match record {
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

/// Execute `pack witness count` — return count of matching witness records.
pub fn execute_count(filters: &WitnessFilters, json_output: bool) -> String {
    let records = read_ledger();
    let count = filter_records(&records, filters, true).len();
    if json_output {
        serde_json::json!({"count": count}).to_string()
    } else {
        format!("{count} witness record(s)")
    }
}

fn filter_records<'a>(
    records: &'a [WitnessRecord],
    filters: &WitnessFilters,
    default_to_pack: bool,
) -> Vec<&'a WitnessRecord> {
    let since = filters.since.as_deref().and_then(parse_bound);
    let until = filters.until.as_deref().and_then(parse_bound);
    let tool_filter = filters
        .tool
        .as_deref()
        .or(default_to_pack.then_some("pack"));

    records
        .iter()
        .filter(|record| match tool_filter {
            Some(tool) => record.tool == tool,
            None => true,
        })
        .filter(|record| match filters.outcome.as_deref() {
            Some(outcome) => record.outcome == outcome,
            None => true,
        })
        .filter(|record| within_bounds(record, since.as_ref(), until.as_ref()))
        .filter(|record| match filters.input_hash.as_deref() {
            Some(input_hash) => record.inputs.iter().any(|input| {
                input
                    .hash
                    .as_deref()
                    .is_some_and(|hash| hash.contains(input_hash))
            }),
            None => true,
        })
        .collect()
}

fn parse_bound(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn within_bounds(
    record: &WitnessRecord,
    since: Option<&DateTime<Utc>>,
    until: Option<&DateTime<Utc>>,
) -> bool {
    if since.is_none() && until.is_none() {
        return true;
    }

    let Some(ts) = parse_bound(&record.ts) else {
        return false;
    };

    if let Some(since) = since {
        if ts < *since {
            return false;
        }
    }

    if let Some(until) = until {
        if ts > *until {
            return false;
        }
    }

    true
}

fn filters_active(filters: &WitnessFilters) -> bool {
    filters.tool.is_some()
        || filters.since.is_some()
        || filters.until.is_some()
        || filters.outcome.is_some()
        || filters.input_hash.is_some()
}

fn format_record_human(record: &WitnessRecord) -> String {
    let ts = if record.ts.is_empty() {
        "-"
    } else {
        &record.ts
    };
    let pack_id = record
        .pack_id
        .as_deref()
        .or_else(|| record.params.get("pack_id").and_then(Value::as_str))
        .unwrap_or("-");

    if let Some(command) = record
        .command
        .as_deref()
        .or_else(|| record.params.get("command").and_then(Value::as_str))
    {
        format!("{ts} {command} {} {pack_id}", record.outcome)
    } else {
        format!("{ts} {} {} {pack_id}", record.tool, record.outcome)
    }
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
        let result = execute_query(&WitnessFilters::default(), false);
        assert_eq!(result, "No witness records found.");
        let json_result = execute_query(&WitnessFilters::default(), true);
        assert_eq!(json_result, "[]");
        teardown();
    }

    #[test]
    fn query_returns_records() {
        let _tmp = setup_ledger();
        let r = WitnessRecord::new(
            "seal",
            vec![crate::witness::WitnessRecord::input(
                "data.json",
                Some("sha256:abc".to_string()),
                Some(10),
            )],
            "PACK_CREATED",
            0,
            serde_json::Map::new(),
            b"PACK_CREATED sha256:abc\n/tmp/out\n",
            Some("sha256:abc".to_string()),
        );
        append_witness(&r).unwrap();

        let result = execute_query(&WitnessFilters::default(), false);
        assert!(result.contains("seal"));
        assert!(result.contains("PACK_CREATED"));

        let json_result = execute_query(&WitnessFilters::default(), true);
        let parsed: Vec<WitnessRecord> = serde_json::from_str(&json_result).unwrap();
        assert_eq!(parsed.len(), 1);
        teardown();
    }

    #[test]
    fn last_returns_most_recent() {
        let _tmp = setup_ledger();
        let r1 = WitnessRecord::new(
            "seal",
            Vec::new(),
            "PACK_CREATED",
            0,
            serde_json::Map::new(),
            b"PACK_CREATED sha256:aaa\n/tmp/a\n",
            None,
        );
        let r2 = WitnessRecord::new(
            "verify",
            Vec::new(),
            "OK",
            0,
            serde_json::Map::new(),
            b"pack verify: OK\n",
            None,
        );
        append_witness(&r1).unwrap();
        append_witness(&r2).unwrap();

        let result = execute_last(false);
        assert!(result.contains("verify"));
        assert!(result.contains("OK"));

        let json_result = execute_last(true);
        let parsed: WitnessRecord = serde_json::from_str(&json_result).unwrap();
        assert_eq!(parsed.command.as_deref(), Some("verify"));
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
        let r1 = WitnessRecord::new(
            "seal",
            Vec::new(),
            "PACK_CREATED",
            0,
            serde_json::Map::new(),
            b"PACK_CREATED sha256:aaa\n/tmp/a\n",
            None,
        );
        let r2 = WitnessRecord::new(
            "verify",
            Vec::new(),
            "OK",
            0,
            serde_json::Map::new(),
            b"pack verify: OK\n",
            None,
        );
        append_witness(&r1).unwrap();
        append_witness(&r2).unwrap();

        let result = execute_count(&WitnessFilters::default(), false);
        assert_eq!(result, "2 witness record(s)");

        let json_result = execute_count(&WitnessFilters::default(), true);
        let parsed: serde_json::Value = serde_json::from_str(&json_result).unwrap();
        assert_eq!(parsed["count"], 2);
        teardown();
    }

    #[test]
    fn count_empty_ledger() {
        let _tmp = setup_ledger();
        let result = execute_count(&WitnessFilters::default(), false);
        assert_eq!(result, "0 witness record(s)");
        teardown();
    }

    #[test]
    fn query_filters_default_to_pack_and_can_target_other_tools() {
        let _tmp = setup_ledger();
        let ledger_path = witness_ledger_path();
        std::fs::write(
            &ledger_path,
            concat!(
                r#"{"tool":"hash","version":"0.2.0","outcome":"OK","exit_code":0,"ts":"2026-01-15T10:00:00Z"}"#,
                "\n",
                r#"{"tool":"pack","version":"0.2.0","command":"verify","outcome":"OK","exit_code":0,"ts":"2026-01-15T10:01:00Z"}"#,
                "\n"
            ),
        )
        .unwrap();

        let default_json = execute_query(&WitnessFilters::default(), true);
        let default_records: Vec<WitnessRecord> = serde_json::from_str(&default_json).unwrap();
        assert_eq!(default_records.len(), 1);
        assert_eq!(default_records[0].tool, "pack");

        let hash_json = execute_query(
            &WitnessFilters {
                tool: Some("hash".to_string()),
                ..WitnessFilters::default()
            },
            true,
        );
        let hash_records: Vec<WitnessRecord> = serde_json::from_str(&hash_json).unwrap();
        assert_eq!(hash_records.len(), 1);
        assert_eq!(hash_records[0].tool, "hash");
        teardown();
    }

    #[test]
    fn query_filters_by_bounds_outcome_and_input_hash() {
        let _tmp = setup_ledger();
        let mut late = WitnessRecord::new(
            "seal",
            vec![crate::witness::WitnessRecord::input(
                "data.json",
                Some("sha256:bbb".to_string()),
                Some(9),
            )],
            "PACK_CREATED",
            0,
            serde_json::Map::new(),
            b"PACK_CREATED sha256:bbb\n/tmp/b\n",
            Some("sha256:bbb".to_string()),
        );
        late.ts = "2026-01-15T10:05:00Z".to_string();

        let mut early = WitnessRecord::new(
            "seal",
            vec![crate::witness::WitnessRecord::input(
                "data.json",
                Some("sha256:aaa".to_string()),
                Some(9),
            )],
            "REFUSAL",
            2,
            serde_json::Map::new(),
            br#"{"outcome":"REFUSAL"}"#,
            None,
        );
        early.ts = "2026-01-15T10:00:00Z".to_string();

        append_witness(&early).unwrap();
        append_witness(&late).unwrap();

        let json_result = execute_query(
            &WitnessFilters {
                since: Some("2026-01-15T10:01:00Z".to_string()),
                until: Some("2026-01-15T10:10:00Z".to_string()),
                outcome: Some("PACK_CREATED".to_string()),
                input_hash: Some("sha256:bbb".to_string()),
                ..WitnessFilters::default()
            },
            true,
        );
        let records: Vec<WitnessRecord> = serde_json::from_str(&json_result).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].outcome, "PACK_CREATED");
        teardown();
    }

    #[test]
    fn legacy_pack_lines_remain_queryable() {
        let _tmp = setup_ledger();
        let ledger_path = witness_ledger_path();
        std::fs::write(
            &ledger_path,
            r#"{"version":"witness.v0","tool":"pack","command":"seal","outcome":"PACK_CREATED","pack_id":"sha256:legacy","timestamp":"2026-01-15T10:00:00.000Z"}"#,
        )
        .unwrap();

        let result = execute_query(&WitnessFilters::default(), false);
        assert!(result.contains("seal"));
        assert!(result.contains("sha256:legacy"));
        teardown();
    }
}
