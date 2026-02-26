use std::collections::HashSet;
use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::seal::collect::is_safe_member_path;
use crate::seal::manifest::Manifest;

use super::report::{InvalidFinding, VerifyChecks};
use super::schema::validate_schemas;

/// Run all integrity checks on a parsed manifest against its pack directory.
///
/// Returns (checks, findings). If findings is empty, the pack is OK.
pub fn run_checks(manifest: &Manifest, pack_dir: &Path) -> (VerifyChecks, Vec<InvalidFinding>) {
    let mut checks = VerifyChecks {
        manifest_parse: true, // Already parsed if we got here
        ..Default::default()
    };
    let mut findings = Vec::new();

    // Check 1: member_count consistency
    checks.member_count = manifest.member_count == manifest.members.len();
    if !checks.member_count {
        findings.push(InvalidFinding {
            code: "MEMBER_COUNT_MISMATCH".to_string(),
            path: None,
            expected: Some(manifest.member_count.to_string()),
            actual: Some(manifest.members.len().to_string()),
        });
    }

    // Check 2: member paths — unique, not reserved, safe
    let mut path_ok = true;
    let mut seen_paths = HashSet::new();
    for member in &manifest.members {
        // Reserved path check
        if member.path == "manifest.json" {
            findings.push(InvalidFinding {
                code: "RESERVED_MEMBER_PATH".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            path_ok = false;
        }

        // Duplicate path check
        if !seen_paths.insert(&member.path) {
            findings.push(InvalidFinding {
                code: "DUPLICATE_MEMBER_PATH".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            path_ok = false;
        }

        // Safe path check
        if !is_safe_member_path(&member.path) {
            findings.push(InvalidFinding {
                code: "UNSAFE_MEMBER_PATH".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            path_ok = false;
        }
    }
    checks.member_paths = path_ok;

    // Check 3: each member exists as regular non-symlink file, and hash matches
    let mut hashes_ok = true;
    for member in &manifest.members {
        let member_path = pack_dir.join(&member.path);

        // Check exists
        if !member_path.exists() {
            findings.push(InvalidFinding {
                code: "MISSING_MEMBER".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            });
            hashes_ok = false;
            continue;
        }

        // Check symlink
        if let Ok(meta) = fs::symlink_metadata(&member_path) {
            if meta.is_symlink() {
                findings.push(InvalidFinding {
                    code: "NON_REGULAR_MEMBER".to_string(),
                    path: Some(member.path.clone()),
                    expected: None,
                    actual: None,
                });
                hashes_ok = false;
                continue;
            }
            if !meta.is_file() {
                findings.push(InvalidFinding {
                    code: "NON_REGULAR_MEMBER".to_string(),
                    path: Some(member.path.clone()),
                    expected: None,
                    actual: None,
                });
                hashes_ok = false;
                continue;
            }
        }

        // Check hash
        if let Ok(content) = fs::read(&member_path) {
            let mut hasher = Sha256::new();
            hasher.update(&content);
            let hash = format!("sha256:{}", hex::encode(hasher.finalize()));
            if hash != member.bytes_hash {
                findings.push(InvalidFinding {
                    code: "HASH_MISMATCH".to_string(),
                    path: Some(member.path.clone()),
                    expected: Some(member.bytes_hash.clone()),
                    actual: Some(hash),
                });
                hashes_ok = false;
            }
        }
    }
    checks.member_hashes = hashes_ok;

    // Check 4: no extra files beyond manifest.json + declared members
    let mut extra_ok = true;
    if let Ok(entries) = fs::read_dir(pack_dir) {
        let declared: HashSet<String> = manifest.members.iter().map(|m| m.path.clone()).collect();

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "manifest.json" {
                continue;
            }
            if entry.path().is_dir() {
                // Check recursively for declared members with dir prefixes
                check_extra_recursive(
                    &entry.path(),
                    &name,
                    &declared,
                    &mut findings,
                    &mut extra_ok,
                );
            } else if !declared.contains(&name) {
                findings.push(InvalidFinding {
                    code: "EXTRA_MEMBER".to_string(),
                    path: Some(name),
                    expected: None,
                    actual: None,
                });
                extra_ok = false;
            }
        }
    }
    checks.extra_members = extra_ok;

    // Check 5: recompute pack_id
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

    // Schema validation: validate known artifact types against local catalog
    let (schema_outcome, schema_findings) = validate_schemas(&manifest.members, pack_dir);
    checks.schema_validation = schema_outcome.as_str().to_string();
    findings.extend(schema_findings);

    (checks, findings)
}

fn check_extra_recursive(
    dir: &Path,
    prefix: &str,
    declared: &HashSet<String>,
    findings: &mut Vec<InvalidFinding>,
    extra_ok: &mut bool,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let relative = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
            if entry.path().is_dir() {
                check_extra_recursive(&entry.path(), &relative, declared, findings, extra_ok);
            } else if !declared.contains(&relative) {
                findings.push(InvalidFinding {
                    code: "EXTRA_MEMBER".to_string(),
                    path: Some(relative),
                    expected: None,
                    actual: None,
                });
                *extra_ok = false;
            }
        }
    }
}
