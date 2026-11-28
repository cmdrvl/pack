use std::collections::{HashMap, HashSet};

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::seal::manifest::{is_safe_member_path, Manifest};
use crate::tar;

use super::report::{InvalidFinding, VerifyReport};
use super::schema::validate_schemas_bytes;

/// Verify a deterministic pack archive without touching the filesystem.
///
/// The report and its outcome match the directory verifier. This checks the pack's own manifest,
/// member paths, member bytes, pack id, and compiled-in artifact schemas; it deliberately does
/// not load or match any lockfile against an external input.
pub fn verify_tar_bytes(tar_bytes: &[u8]) -> VerifyReport {
    let entries = match tar::decode_entries(tar_bytes) {
        Ok(entries) => entries,
        Err(refusal) => return VerifyReport::refusal(refusal_json(&refusal)),
    };

    let Some(manifest_entry) = entries.iter().find(|entry| entry.path == "manifest.json") else {
        return VerifyReport::refusal(json!({
            "code": "E_BAD_PACK",
            "message": "Archive does not contain manifest.json"
        }));
    };
    let manifest: Manifest = match serde_json::from_slice(&manifest_entry.bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            return VerifyReport::refusal(json!({
                "code": "E_BAD_PACK",
                "message": format!("Invalid manifest.json: {error}")
            }));
        }
    };
    if manifest.version != "pack.v0" {
        return VerifyReport::refusal(json!({
            "code": "E_BAD_PACK",
            "message": format!("Unsupported manifest version: {}", manifest.version)
        }));
    }

    let member_bytes: Vec<(&str, &[u8])> = entries
        .iter()
        .filter(|entry| entry.path != "manifest.json")
        .map(|entry| (entry.path.as_str(), entry.bytes.as_slice()))
        .collect();
    verify_manifest_members(&manifest, &member_bytes)
}

/// Verify an already parsed manifest and its in-memory member bytes.
pub(crate) fn verify_manifest_members(
    manifest: &Manifest,
    member_bytes: &[(&str, &[u8])],
) -> VerifyReport {
    let mut checks = super::report::VerifyChecks {
        manifest_parse: true,
        ..Default::default()
    };
    let mut findings = Vec::new();

    checks.member_count = manifest.member_count == manifest.members.len();
    if !checks.member_count {
        findings.push(InvalidFinding {
            code: "MEMBER_COUNT_MISMATCH".to_string(),
            path: None,
            expected: Some(manifest.member_count.to_string()),
            actual: Some(manifest.members.len().to_string()),
        });
    }

    let mut declared_paths = HashSet::new();
    let mut paths_ok = true;
    for member in &manifest.members {
        if member.path == "manifest.json" {
            findings.push(InvalidFinding {
                code: "RESERVED_MEMBER_PATH".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            paths_ok = false;
        }
        if !declared_paths.insert(member.path.as_str()) {
            findings.push(InvalidFinding {
                code: "DUPLICATE_MEMBER_PATH".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            paths_ok = false;
        }
        if !is_safe_member_path(&member.path) {
            findings.push(InvalidFinding {
                code: "UNSAFE_MEMBER_PATH".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            paths_ok = false;
        }
    }
    checks.member_paths = paths_ok;

    let mut actual_paths = HashMap::new();
    let mut extra_ok = true;
    for (path, bytes) in member_bytes {
        if actual_paths.insert(*path, *bytes).is_some() {
            findings.push(InvalidFinding {
                code: "DUPLICATE_MEMBER_PATH".to_string(),
                path: Some((*path).to_string()),
                expected: None,
                actual: None,
            });
            extra_ok = false;
        } else if !declared_paths.contains(path) {
            findings.push(InvalidFinding {
                code: "EXTRA_MEMBER".to_string(),
                path: Some((*path).to_string()),
                expected: None,
                actual: None,
            });
            extra_ok = false;
        }
    }
    for member in &manifest.members {
        let Some(bytes) = actual_paths.get(member.path.as_str()) else {
            findings.push(InvalidFinding {
                code: "MISSING_MEMBER".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            continue;
        };
        let actual_hash = sha256_bytes(bytes);
        if actual_hash != member.bytes_hash {
            findings.push(InvalidFinding {
                code: "HASH_MISMATCH".to_string(),
                path: Some(member.path.clone()),
                expected: Some(member.bytes_hash.clone()),
                actual: Some(actual_hash),
            });
        }
    }
    checks.member_hashes = !findings
        .iter()
        .any(|finding| matches!(finding.code.as_str(), "MISSING_MEMBER" | "HASH_MISMATCH"));
    checks.extra_members = extra_ok;

    let recomputed = manifest.recompute_pack_id();
    checks.pack_id = recomputed == manifest.pack_id;
    if !checks.pack_id {
        findings.push(InvalidFinding {
            code: "PACK_ID_MISMATCH".to_string(),
            path: None,
            expected: Some(manifest.pack_id.clone()),
            actual: Some(recomputed),
        });
    }

    let (schema_outcome, schema_findings) = validate_schemas_bytes(&manifest.members, member_bytes);
    checks.schema_validation = schema_outcome.as_str().to_string();
    findings.extend(schema_findings);

    if findings.is_empty() {
        VerifyReport::ok(manifest.pack_id.clone(), checks)
    } else {
        VerifyReport::invalid(Some(manifest.pack_id.clone()), checks, findings)
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn refusal_json(refusal: &crate::refusal::RefusalEnvelope) -> serde_json::Value {
    json!({
        "code": refusal.refusal.code,
        "message": refusal.refusal.message,
        "detail": refusal.refusal.detail,
    })
}
