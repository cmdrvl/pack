use std::collections::HashSet;

use serde_json::json;

use super::collect::MemberCandidate;
use crate::refusal::{RefusalCode, RefusalEnvelope};

/// Reserved member path that cannot be used by any input artifact.
pub const RESERVED_MANIFEST_PATH: &str = "manifest.json";

/// Check the resolved member set for path collisions and reserved-name violations.
///
/// Returns `Ok(())` if all member paths are unique and none use reserved names.
/// Returns `Err` with `E_DUPLICATE` refusal containing collision details.
pub fn check_collisions(candidates: &[MemberCandidate]) -> Result<(), Box<RefusalEnvelope>> {
    let mut seen = HashSet::new();

    for candidate in candidates {
        // Check reserved path
        if candidate.member_path == RESERVED_MANIFEST_PATH {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::Duplicate,
                Some("Reserved member path collision".to_string()),
                Some(json!({
                    "path": RESERVED_MANIFEST_PATH,
                    "sources": [candidate.source.display().to_string()]
                })),
            )));
        }

        // Check duplicate
        if !seen.insert(&candidate.member_path) {
            // Find all sources with this path
            let sources: Vec<String> = candidates
                .iter()
                .filter(|c| c.member_path == candidate.member_path)
                .map(|c| c.source.display().to_string())
                .collect();

            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::Duplicate,
                Some("Resolved member path collision".to_string()),
                Some(json!({
                    "path": candidate.member_path,
                    "sources": sources
                })),
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn candidate(source: &str, member_path: &str) -> MemberCandidate {
        MemberCandidate {
            source: PathBuf::from(source),
            member_path: member_path.to_string(),
        }
    }

    #[test]
    fn no_collision_passes() {
        let candidates = vec![
            candidate("/a/one.json", "one.json"),
            candidate("/b/two.json", "two.json"),
        ];
        assert!(check_collisions(&candidates).is_ok());
    }

    #[test]
    fn empty_candidates_passes() {
        assert!(check_collisions(&[]).is_ok());
    }

    #[test]
    fn duplicate_path_returns_e_duplicate() {
        let candidates = vec![
            candidate("/a/report.json", "report.json"),
            candidate("/b/report.json", "report.json"),
        ];
        let err = check_collisions(&candidates).unwrap_err();
        assert_eq!(err.refusal.code, "E_DUPLICATE");
        let detail = err.refusal.detail.as_ref().unwrap();
        assert_eq!(detail["path"], "report.json");
        let sources = detail["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn reserved_manifest_path_returns_e_duplicate() {
        let candidates = vec![candidate("/a/manifest.json", "manifest.json")];
        let err = check_collisions(&candidates).unwrap_err();
        assert_eq!(err.refusal.code, "E_DUPLICATE");
        let detail = err.refusal.detail.as_ref().unwrap();
        assert_eq!(detail["path"], "manifest.json");
    }

    #[test]
    fn collision_detail_includes_sources() {
        let candidates = vec![
            candidate("/x/data.json", "data.json"),
            candidate("/y/data.json", "data.json"),
            candidate("/z/other.json", "other.json"),
        ];
        let err = check_collisions(&candidates).unwrap_err();
        let detail = err.refusal.detail.as_ref().unwrap();
        let sources = detail["sources"].as_array().unwrap();
        assert!(sources.iter().any(|s| s.as_str().unwrap().contains("/x/")));
        assert!(sources.iter().any(|s| s.as_str().unwrap().contains("/y/")));
    }

    #[test]
    fn no_partial_output_on_collision() {
        // Collision is detected before any copy would happen,
        // so this is a contract test: the function returns Err, not Ok.
        let candidates = vec![
            candidate("/a/f.json", "f.json"),
            candidate("/b/f.json", "f.json"),
        ];
        assert!(check_collisions(&candidates).is_err());
    }
}
