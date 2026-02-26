use chrono::Utc;
use serde::{Deserialize, Serialize};

/// A witness.v0 record appended to the witness ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessRecord {
    pub version: String,
    pub tool: String,
    pub command: String,
    pub outcome: String,
    pub pack_id: Option<String>,
    pub timestamp: String,
}

impl WitnessRecord {
    pub fn new(command: &str, outcome: &str, pack_id: Option<String>) -> Self {
        Self {
            version: "witness.v0".to_string(),
            tool: "pack".to_string(),
            command: command.to_string(),
            outcome: outcome.to_string(),
            pack_id,
            timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    }
}
