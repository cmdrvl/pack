use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifyOutcome {
    OK,
    INVALID,
    REFUSAL,
}

impl std::fmt::Display for VerifyOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyOutcome::OK => write!(f, "OK"),
            VerifyOutcome::INVALID => write!(f, "INVALID"),
            VerifyOutcome::REFUSAL => write!(f, "REFUSAL"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyChecks {
    pub manifest_parse: bool,
    pub member_count: bool,
    pub member_paths: bool,
    pub extra_members: bool,
    pub member_hashes: bool,
    pub pack_id: bool,
    pub schema_validation: String,
}

impl Default for VerifyChecks {
    fn default() -> Self {
        Self {
            manifest_parse: false,
            member_count: false,
            member_paths: false,
            extra_members: false,
            member_hashes: false,
            pack_id: false,
            schema_validation: "skipped".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidFinding {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub version: String,
    pub outcome: VerifyOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_id: Option<String>,
    pub checks: VerifyChecks,
    pub invalid: Vec<InvalidFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<serde_json::Value>,
}

impl VerifyReport {
    pub fn ok(pack_id: String, checks: VerifyChecks) -> Self {
        Self {
            version: "pack.verify.v0".to_string(),
            outcome: VerifyOutcome::OK,
            pack_id: Some(pack_id),
            checks,
            invalid: vec![],
            refusal: None,
        }
    }

    pub fn invalid(
        pack_id: Option<String>,
        checks: VerifyChecks,
        findings: Vec<InvalidFinding>,
    ) -> Self {
        Self {
            version: "pack.verify.v0".to_string(),
            outcome: VerifyOutcome::INVALID,
            pack_id,
            checks,
            invalid: findings,
            refusal: None,
        }
    }

    pub fn refusal(reason: serde_json::Value) -> Self {
        Self {
            version: "pack.verify.v0".to_string(),
            outcome: VerifyOutcome::REFUSAL,
            pack_id: None,
            checks: VerifyChecks::default(),
            invalid: vec![],
            refusal: Some(reason),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("verify report serialization cannot fail")
    }

    pub fn to_human(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("pack verify: {}", self.outcome));
        if let Some(id) = &self.pack_id {
            lines.push(format!("  pack_id: {id}"));
        }
        if !self.invalid.is_empty() {
            lines.push("  findings:".to_string());
            for f in &self.invalid {
                let mut entry = format!("    - {}", f.code);
                if let Some(p) = &f.path {
                    entry.push_str(&format!(" ({p})"));
                }
                lines.push(entry);
            }
        }
        if let Some(r) = &self.refusal {
            lines.push(format!("  refusal: {r}"));
        }
        lines.join("\n")
    }
}
