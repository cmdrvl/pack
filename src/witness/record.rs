use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessInput {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

/// A witness.v0 record appended to the witness ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WitnessRecord {
    #[serde(default)]
    pub id: String,
    pub tool: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub binary_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<WitnessInput>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    pub outcome: String,
    #[serde(default)]
    pub exit_code: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output_hash: String,
    #[serde(default, alias = "timestamp")]
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_id: Option<String>,
}

impl WitnessRecord {
    pub fn input(
        path: impl Into<String>,
        hash: Option<String>,
        bytes: Option<u64>,
    ) -> WitnessInput {
        WitnessInput {
            path: path.into(),
            hash,
            bytes,
        }
    }

    pub fn new(
        command: impl Into<String>,
        inputs: Vec<WitnessInput>,
        outcome: impl Into<String>,
        exit_code: u8,
        params: Map<String, Value>,
        output_bytes: &[u8],
        pack_id: Option<String>,
    ) -> Self {
        Self {
            id: String::new(),
            tool: "pack".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            command: Some(command.into()),
            binary_hash: hash_self()
                .map(|value| format!("blake3:{value}"))
                .unwrap_or_default(),
            inputs,
            params,
            outcome: outcome.into(),
            exit_code,
            output_hash: format!("blake3:{}", blake3::hash(output_bytes).to_hex()),
            ts: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            pack_id,
        }
    }

    pub fn compute_id(&mut self) {
        self.id.clear();
        self.id = format!(
            "blake3:{}",
            blake3::hash(canonical_json(self).as_bytes()).to_hex()
        );
    }
}

pub fn canonical_json(record: &WitnessRecord) -> String {
    let value = serde_json::to_value(record).expect("WitnessRecord should serialize");
    serde_json::to_string(&value).expect("WitnessRecord JSON should encode")
}

fn hash_self() -> Result<String, std::io::Error> {
    let path = std::env::current_exe()?;
    let bytes = std::fs::read(path)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
