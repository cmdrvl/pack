use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::json;

use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::manifest::Manifest;
use crate::verify::has_schema_for_version;

const INSPECT_VERSION: &str = "pack.inspect.v0";
const LARGEST_MEMBER_LIMIT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectReport {
    pub version: String,
    pub pack_dir: String,
    pub pack_id: String,
    pub created: String,
    pub tool_version: String,
    pub note: Option<String>,
    pub member_count: usize,
    pub type_counts: Vec<TypeCount>,
    pub largest_members: Vec<MemberInspect>,
    pub schema_validation: SchemaEligibility,
    pub integrity: IntegrityNotice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TypeCount {
    #[serde(rename = "type")]
    pub member_type: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemberInspect {
    pub path: String,
    #[serde(rename = "type")]
    pub member_type: String,
    pub artifact_version: Option<String>,
    pub bytes_hash: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchemaEligibility {
    pub eligible_count: usize,
    pub skipped_count: usize,
    pub eligible_types: Vec<TypeCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrityNotice {
    pub verified: bool,
    pub message: String,
    pub next_command: String,
}

/// Execute read-only pack metadata inspection.
///
/// `inspect` intentionally does not run full integrity checks and does not
/// append witness records. It is a metadata convenience, not a verifier.
pub fn execute_inspect(pack_dir: &Path, json_output: bool) -> (String, u8) {
    match inspect_pack(pack_dir) {
        Ok(report) => {
            let output = if json_output {
                report.to_json()
            } else {
                report.to_human()
            };
            (output, 0)
        }
        Err(envelope) => {
            let output = if json_output {
                envelope.to_json()
            } else {
                refusal_to_human(&envelope)
            };
            (output, 2)
        }
    }
}

fn inspect_pack(pack_dir: &Path) -> Result<InspectReport, Box<RefusalEnvelope>> {
    let manifest_path = pack_dir.join("manifest.json");
    let manifest_content = fs::read_to_string(&manifest_path).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Cannot read manifest.json: {error}")),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
            })),
        ))
    })?;

    let manifest: Manifest = serde_json::from_str(&manifest_content).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Invalid manifest.json: {error}")),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
            })),
        ))
    })?;

    if manifest.version != "pack.v0" {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Unsupported manifest version: {}",
                manifest.version
            )),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
                "version": manifest.version,
            })),
        )));
    }

    let type_counts = type_counts(&manifest);
    let largest_members = largest_members(pack_dir, &manifest);
    let schema_validation = schema_eligibility(&manifest);

    Ok(InspectReport {
        version: INSPECT_VERSION.to_string(),
        pack_dir: pack_dir.display().to_string(),
        pack_id: manifest.pack_id,
        created: manifest.created,
        tool_version: manifest.tool_version,
        note: manifest.note,
        member_count: manifest.member_count,
        type_counts,
        largest_members,
        schema_validation,
        integrity: IntegrityNotice {
            verified: false,
            message: "inspect reads metadata only; it does not verify hashes, closed-set membership, or pack_id integrity".to_string(),
            next_command: format!("pack verify {}", pack_dir.display()),
        },
    })
}

impl InspectReport {
    fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("inspect report serialization cannot fail")
    }

    fn to_human(&self) -> String {
        let mut lines = Vec::new();
        lines.push("pack inspect: METADATA".to_string());
        lines.push(format!("  pack_id: {}", self.pack_id));
        lines.push(format!("  created: {}", self.created));
        lines.push(format!("  tool_version: {}", self.tool_version));
        if let Some(note) = &self.note {
            lines.push(format!("  note: {note}"));
        }
        lines.push(format!("  member_count: {}", self.member_count));
        lines.push(
            "  integrity: not verified; run pack verify to check hashes and closed-set membership"
                .to_string(),
        );
        lines.push("  member types:".to_string());
        for type_count in &self.type_counts {
            lines.push(format!(
                "    - {}: {}",
                type_count.member_type, type_count.count
            ));
        }
        lines.push("  largest members:".to_string());
        for member in &self.largest_members {
            let size = member
                .size_bytes
                .map(|bytes| format!("{bytes} bytes"))
                .unwrap_or_else(|| "size unavailable".to_string());
            lines.push(format!(
                "    - {} ({}, {})",
                member.path, size, member.member_type
            ));
        }
        lines.push(format!(
            "  schema eligibility: {} eligible, {} skipped",
            self.schema_validation.eligible_count, self.schema_validation.skipped_count
        ));
        lines.join("\n")
    }
}

fn type_counts(manifest: &Manifest) -> Vec<TypeCount> {
    let mut counts = BTreeMap::new();
    for member in &manifest.members {
        *counts.entry(member.member_type.clone()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(member_type, count)| TypeCount { member_type, count })
        .collect()
}

fn largest_members(pack_dir: &Path, manifest: &Manifest) -> Vec<MemberInspect> {
    let mut members: Vec<MemberInspect> = manifest
        .members
        .iter()
        .map(|member| MemberInspect {
            path: member.path.clone(),
            member_type: member.member_type.clone(),
            artifact_version: member.artifact_version.clone(),
            bytes_hash: member.bytes_hash.clone(),
            size_bytes: fs::metadata(pack_dir.join(&member.path))
                .ok()
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len()),
        })
        .collect();

    members.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.path.cmp(&b.path))
    });
    members.truncate(LARGEST_MEMBER_LIMIT);
    members
}

fn schema_eligibility(manifest: &Manifest) -> SchemaEligibility {
    let mut eligible_types = BTreeMap::new();
    let mut eligible_count = 0;

    for member in &manifest.members {
        let eligible = member
            .artifact_version
            .as_deref()
            .is_some_and(has_schema_for_version);
        if eligible {
            eligible_count += 1;
            *eligible_types
                .entry(member.member_type.clone())
                .or_insert(0) += 1;
        }
    }

    SchemaEligibility {
        eligible_count,
        skipped_count: manifest.members.len().saturating_sub(eligible_count),
        eligible_types: eligible_types
            .into_iter()
            .map(|(member_type, count)| TypeCount { member_type, count })
            .collect(),
    }
}

fn refusal_to_human(envelope: &RefusalEnvelope) -> String {
    format!(
        "pack inspect: REFUSAL\n  code: {}\n  message: {}",
        envelope.refusal.code, envelope.refusal.message
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::command::execute_seal;

    fn create_test_pack() -> (tempfile::TempDir, std::path::PathBuf, String) {
        let tmp = tempfile::tempdir().unwrap();
        let small = tmp.path().join("a.lock.json");
        let large = tmp.path().join("b.report.json");
        fs::write(&small, r#"{"version":"lock.v0","rows":5}"#).unwrap();
        fs::write(
            &large,
            r#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE","padding":"abcdef"}"#,
        )
        .unwrap();
        let pack_dir = tmp.path().join("pack");
        let result = execute_seal(
            &[small, large],
            Some(&pack_dir),
            Some("inspect me".to_string()),
            None,
        )
        .unwrap();
        (tmp, pack_dir, result.pack_id)
    }

    #[test]
    fn inspect_valid_pack_json_is_deterministic_metadata() {
        let (_tmp, pack_dir, pack_id) = create_test_pack();

        let (output, code) = execute_inspect(&pack_dir, true);

        assert_eq!(code, 0);
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(report["version"], INSPECT_VERSION);
        assert_eq!(report["pack_id"], pack_id);
        assert_eq!(report["member_count"], 2);
        assert_eq!(report["integrity"]["verified"], false);
        assert_eq!(report["schema_validation"]["eligible_count"], 2);
        assert_eq!(report["type_counts"][0]["type"], "lockfile");
        assert_eq!(report["largest_members"][0]["path"], "b.report.json");
    }

    #[test]
    fn inspect_human_output_avoids_integrity_claim() {
        let (_tmp, pack_dir, pack_id) = create_test_pack();

        let (output, code) = execute_inspect(&pack_dir, false);

        assert_eq!(code, 0);
        assert!(output.contains("pack inspect: METADATA"));
        assert!(output.contains(&format!("pack_id: {pack_id}")));
        assert!(output.contains("integrity: not verified"));
        assert!(output.contains("schema eligibility: 2 eligible"));
    }

    #[test]
    fn inspect_malformed_manifest_refuses() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("manifest.json"), "not-json").unwrap();

        let (output, code) = execute_inspect(tmp.path(), true);

        assert_eq!(code, 2);
        let refusal: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(refusal["outcome"], "REFUSAL");
        assert_eq!(refusal["refusal"]["code"], "E_BAD_PACK");
    }
}
