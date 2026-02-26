use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::seal::manifest::{Manifest, Member};

/// A single difference between two packs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffEntry {
    pub kind: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b_hash: Option<String>,
}

/// Result of comparing two pack manifests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    pub version: String,
    pub outcome: String,
    pub a_pack_id: String,
    pub b_pack_id: String,
    pub added: Vec<DiffEntry>,
    pub removed: Vec<DiffEntry>,
    pub changed: Vec<DiffEntry>,
    pub unchanged: usize,
}

impl DiffReport {
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("diff report serialization cannot fail")
    }

    pub fn to_human(&self) -> String {
        let mut lines = Vec::new();
        if self.has_changes() {
            lines.push("pack diff: CHANGES".to_string());
        } else {
            lines.push("pack diff: NO_CHANGES".to_string());
        }
        lines.push(format!("  a: {}", self.a_pack_id));
        lines.push(format!("  b: {}", self.b_pack_id));

        if !self.added.is_empty() {
            lines.push(format!("  added: {}", self.added.len()));
            for e in &self.added {
                lines.push(format!("    + {}", e.path));
            }
        }
        if !self.removed.is_empty() {
            lines.push(format!("  removed: {}", self.removed.len()));
            for e in &self.removed {
                lines.push(format!("    - {}", e.path));
            }
        }
        if !self.changed.is_empty() {
            lines.push(format!("  changed: {}", self.changed.len()));
            for e in &self.changed {
                lines.push(format!("    ~ {}", e.path));
            }
        }
        if self.unchanged > 0 {
            lines.push(format!("  unchanged: {}", self.unchanged));
        }

        lines.join("\n")
    }
}

/// Compare two manifests and produce a deterministic diff report.
pub fn compare_manifests(a: &Manifest, b: &Manifest) -> DiffReport {
    let a_members: BTreeMap<&str, &Member> =
        a.members.iter().map(|m| (m.path.as_str(), m)).collect();
    let b_members: BTreeMap<&str, &Member> =
        b.members.iter().map(|m| (m.path.as_str(), m)).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0usize;

    // Find removed and changed (in A but not in B, or different hash)
    for (path, a_member) in &a_members {
        match b_members.get(path) {
            None => {
                removed.push(DiffEntry {
                    kind: "removed".to_string(),
                    path: path.to_string(),
                    a_hash: Some(a_member.bytes_hash.clone()),
                    b_hash: None,
                });
            }
            Some(b_member) => {
                if a_member.bytes_hash != b_member.bytes_hash {
                    changed.push(DiffEntry {
                        kind: "changed".to_string(),
                        path: path.to_string(),
                        a_hash: Some(a_member.bytes_hash.clone()),
                        b_hash: Some(b_member.bytes_hash.clone()),
                    });
                } else {
                    unchanged += 1;
                }
            }
        }
    }

    // Find added (in B but not in A)
    for (path, b_member) in &b_members {
        if !a_members.contains_key(path) {
            added.push(DiffEntry {
                kind: "added".to_string(),
                path: path.to_string(),
                a_hash: None,
                b_hash: Some(b_member.bytes_hash.clone()),
            });
        }
    }

    let outcome = if added.is_empty() && removed.is_empty() && changed.is_empty() {
        "NO_CHANGES"
    } else {
        "CHANGES"
    };

    DiffReport {
        version: "pack.diff.v0".to_string(),
        outcome: outcome.to_string(),
        a_pack_id: a.pack_id.clone(),
        b_pack_id: b.pack_id.clone(),
        added,
        removed,
        changed,
        unchanged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::manifest::Member;

    fn member(path: &str, hash: &str) -> Member {
        Member {
            path: path.to_string(),
            bytes_hash: format!("sha256:{hash}"),
            member_type: "other".to_string(),
            artifact_version: None,
        }
    }

    fn manifest(pack_id: &str, members: Vec<Member>) -> Manifest {
        let member_count = members.len();
        Manifest {
            version: "pack.v0".to_string(),
            pack_id: pack_id.to_string(),
            created: "2026-01-15T00:00:00Z".to_string(),
            note: None,
            tool_version: "0.1.0".to_string(),
            members,
            member_count,
        }
    }

    #[test]
    fn identical_packs_no_changes() {
        let a = manifest(
            "sha256:aaa",
            vec![member("x.json", "111"), member("y.json", "222")],
        );
        let b = manifest(
            "sha256:bbb",
            vec![member("x.json", "111"), member("y.json", "222")],
        );
        let report = compare_manifests(&a, &b);
        assert_eq!(report.outcome, "NO_CHANGES");
        assert!(!report.has_changes());
        assert_eq!(report.unchanged, 2);
    }

    #[test]
    fn added_member() {
        let a = manifest("sha256:aaa", vec![member("x.json", "111")]);
        let b = manifest(
            "sha256:bbb",
            vec![member("x.json", "111"), member("y.json", "222")],
        );
        let report = compare_manifests(&a, &b);
        assert_eq!(report.outcome, "CHANGES");
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.added[0].path, "y.json");
        assert_eq!(report.unchanged, 1);
    }

    #[test]
    fn removed_member() {
        let a = manifest(
            "sha256:aaa",
            vec![member("x.json", "111"), member("y.json", "222")],
        );
        let b = manifest("sha256:bbb", vec![member("x.json", "111")]);
        let report = compare_manifests(&a, &b);
        assert_eq!(report.outcome, "CHANGES");
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].path, "y.json");
    }

    #[test]
    fn changed_member() {
        let a = manifest("sha256:aaa", vec![member("x.json", "111")]);
        let b = manifest("sha256:bbb", vec![member("x.json", "999")]);
        let report = compare_manifests(&a, &b);
        assert_eq!(report.outcome, "CHANGES");
        assert_eq!(report.changed.len(), 1);
        assert_eq!(report.changed[0].path, "x.json");
        assert_eq!(report.changed[0].a_hash.as_deref(), Some("sha256:111"));
        assert_eq!(report.changed[0].b_hash.as_deref(), Some("sha256:999"));
    }

    #[test]
    fn mixed_changes() {
        let a = manifest(
            "sha256:aaa",
            vec![
                member("keep.json", "111"),
                member("change.json", "222"),
                member("remove.json", "333"),
            ],
        );
        let b = manifest(
            "sha256:bbb",
            vec![
                member("keep.json", "111"),
                member("change.json", "999"),
                member("add.json", "444"),
            ],
        );
        let report = compare_manifests(&a, &b);
        assert_eq!(report.outcome, "CHANGES");
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.changed.len(), 1);
        assert_eq!(report.unchanged, 1);
    }

    #[test]
    fn deterministic_ordering() {
        let a = manifest(
            "sha256:aaa",
            vec![member("z.json", "111"), member("a.json", "222")],
        );
        let b = manifest("sha256:bbb", vec![]);
        let report = compare_manifests(&a, &b);
        // BTreeMap ensures alphabetical ordering
        assert_eq!(report.removed[0].path, "a.json");
        assert_eq!(report.removed[1].path, "z.json");
    }

    #[test]
    fn human_output_format() {
        let a = manifest("sha256:aaa", vec![member("x.json", "111")]);
        let b = manifest(
            "sha256:bbb",
            vec![member("x.json", "111"), member("y.json", "222")],
        );
        let report = compare_manifests(&a, &b);
        let human = report.to_human();
        assert!(human.contains("CHANGES"));
        assert!(human.contains("+ y.json"));
    }

    #[test]
    fn json_output_roundtrips() {
        let a = manifest("sha256:aaa", vec![member("x.json", "111")]);
        let b = manifest("sha256:bbb", vec![member("x.json", "999")]);
        let report = compare_manifests(&a, &b);
        let json = report.to_json();
        let parsed: DiffReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.outcome, "CHANGES");
        assert_eq!(parsed.changed.len(), 1);
    }
}
