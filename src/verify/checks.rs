use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use sha2::{Digest, Sha256};

use crate::parallel::worker_count;
use crate::seal::collect::is_safe_member_path;
use crate::seal::manifest::{Manifest, Member};

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

    // Check 3: each member exists as regular non-symlink file, and hash matches.
    let (hashes_ok, hash_findings) = member_hash_findings(manifest, pack_dir);
    findings.extend(hash_findings);
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

fn member_hash_findings(manifest: &Manifest, pack_dir: &Path) -> (bool, Vec<InvalidFinding>) {
    member_hash_findings_with_workers(manifest, pack_dir, worker_count(manifest.members.len()))
}

fn member_hash_findings_with_workers(
    manifest: &Manifest,
    pack_dir: &Path,
    workers: usize,
) -> (bool, Vec<InvalidFinding>) {
    if workers <= 1 || manifest.members.len() <= 1 {
        return member_hash_findings_sequential(&manifest.members, pack_dir);
    }

    let jobs = Arc::new(Mutex::new(
        (0..manifest.members.len()).collect::<VecDeque<_>>(),
    ));
    let results = Arc::new(Mutex::new(
        (0..manifest.members.len())
            .map(|_| None)
            .collect::<Vec<_>>(),
    ));

    thread::scope(|scope| {
        for _ in 0..workers {
            let jobs = Arc::clone(&jobs);
            let results = Arc::clone(&results);
            scope.spawn(move || loop {
                let index = {
                    let mut jobs = jobs.lock().expect("verify job queue poisoned");
                    jobs.pop_front()
                };
                let Some(index) = index else {
                    break;
                };
                let result = check_one_member_hash(&manifest.members[index], pack_dir);
                let mut results = results.lock().expect("verify results poisoned");
                results[index] = Some(result);
            });
        }
    });

    collect_ordered_hash_results(results)
}

fn member_hash_findings_sequential(
    members: &[Member],
    pack_dir: &Path,
) -> (bool, Vec<InvalidFinding>) {
    let mut ok = true;
    let mut findings = Vec::new();
    for member in members {
        let result = check_one_member_hash(member, pack_dir);
        ok &= result.ok;
        findings.extend(result.findings);
    }
    (ok, findings)
}

#[derive(Debug)]
struct MemberHashResult {
    ok: bool,
    findings: Vec<InvalidFinding>,
}

fn check_one_member_hash(member: &Member, pack_dir: &Path) -> MemberHashResult {
    let member_path = pack_dir.join(&member.path);

    // Check exists.
    if !member_path.exists() {
        return MemberHashResult {
            ok: false,
            findings: vec![InvalidFinding {
                code: "MISSING_MEMBER".to_string(),
                path: Some(member.path.clone()),
                expected: None,
                actual: None,
            }],
        };
    }

    // Check symlink and non-file members.
    if let Ok(meta) = fs::symlink_metadata(&member_path) {
        if meta.is_symlink() || !meta.is_file() {
            return MemberHashResult {
                ok: false,
                findings: vec![InvalidFinding {
                    code: "NON_REGULAR_MEMBER".to_string(),
                    path: Some(member.path.clone()),
                    expected: None,
                    actual: None,
                }],
            };
        }
    }

    // Check hash without reading the full member into memory.
    if let Ok(hash) = hash_file(&member_path) {
        if hash != member.bytes_hash {
            return MemberHashResult {
                ok: false,
                findings: vec![InvalidFinding {
                    code: "HASH_MISMATCH".to_string(),
                    path: Some(member.path.clone()),
                    expected: Some(member.bytes_hash.clone()),
                    actual: Some(hash),
                }],
            };
        }
    }

    MemberHashResult {
        ok: true,
        findings: Vec::new(),
    }
}

fn collect_ordered_hash_results(
    results: Arc<Mutex<Vec<Option<MemberHashResult>>>>,
) -> (bool, Vec<InvalidFinding>) {
    let mut results = Arc::try_unwrap(results)
        .expect("verify results still referenced")
        .into_inner()
        .expect("verify results poisoned");
    let mut ok = true;
    let mut findings = Vec::new();

    for result in results.drain(..) {
        let result = result.expect("verify worker did not fill result");
        ok &= result.ok;
        findings.extend(result.findings);
    }

    (ok, findings)
}

fn hash_file(path: &Path) -> Result<String, io::Error> {
    let mut reader = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::manifest::Manifest;
    use tempfile::TempDir;

    fn hash_bytes(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    fn manifest_for(paths: &[&str], hashes: &[String]) -> Manifest {
        let members = paths
            .iter()
            .zip(hashes)
            .map(|(path, hash)| Member {
                path: (*path).to_string(),
                bytes_hash: hash.clone(),
                member_type: "other".to_string(),
                artifact_version: None,
            })
            .collect::<Vec<_>>();
        let mut manifest = Manifest::new(
            "2026-01-15T10:30:00Z".to_string(),
            None,
            None,
            env!("CARGO_PKG_VERSION").to_string(),
            members,
        );
        manifest.finalize();
        manifest
    }

    #[test]
    fn parallel_hash_findings_match_single_thread_order() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.bin"), b"a").unwrap();
        fs::write(tmp.path().join("b.bin"), b"B-tampered").unwrap();
        fs::write(tmp.path().join("c.bin"), b"C-tampered").unwrap();
        fs::write(tmp.path().join("d.bin"), b"d").unwrap();

        let manifest = manifest_for(
            &["a.bin", "b.bin", "c.bin", "d.bin"],
            &[
                hash_bytes(b"a"),
                hash_bytes(b"b-original"),
                hash_bytes(b"c-original"),
                hash_bytes(b"d"),
            ],
        );

        let single = member_hash_findings_with_workers(&manifest, tmp.path(), 1);
        let parallel = member_hash_findings_with_workers(&manifest, tmp.path(), 4);

        assert_eq!(single.0, parallel.0);
        assert_eq!(single.1.len(), 2);
        assert_eq!(parallel.1.len(), 2);
        let single_paths = single
            .1
            .iter()
            .map(|finding| finding.path.as_deref())
            .collect::<Vec<_>>();
        let parallel_paths = parallel
            .1
            .iter()
            .map(|finding| finding.path.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(single_paths, vec![Some("b.bin"), Some("c.bin")]);
        assert_eq!(single_paths, parallel_paths);
    }
}
