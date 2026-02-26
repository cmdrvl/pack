use serde_json::{json, Value};

/// Return the compiled-in operator manifest for `--describe`.
pub fn operator_json() -> Value {
    json!({
        "name": "pack",
        "schema_version": "operator.v0",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Seal lockfiles, reports, rules, and registry artifacts into one immutable, self-verifiable evidence pack.",
        "output_mode": "mixed",
        "subcommands": {
            "seal": {
                "description": "Seal artifacts into an evidence pack directory",
                "output_mode": "directory_artifact",
                "exit_codes": {
                    "0": "PACK_CREATED",
                    "2": "REFUSAL"
                }
            },
            "verify": {
                "description": "Verify pack integrity (members + pack_id)",
                "output_mode": "report",
                "exit_codes": {
                    "0": "OK",
                    "1": "INVALID",
                    "2": "REFUSAL"
                }
            },
            "diff": {
                "description": "Deterministically diff two packs (deferred in v0.1)",
                "output_mode": "report",
                "status": "deferred",
                "exit_codes": {
                    "0": "NO_CHANGES",
                    "1": "CHANGES",
                    "2": "REFUSAL"
                }
            },
            "push": {
                "description": "Publish a pack to data-fabric (deferred in v0.1)",
                "output_mode": "status",
                "status": "deferred",
                "exit_codes": {
                    "0": "PUBLISHED",
                    "2": "REFUSAL"
                }
            },
            "pull": {
                "description": "Fetch a pack by ID from data-fabric (deferred in v0.1)",
                "output_mode": "status",
                "status": "deferred",
                "exit_codes": {
                    "0": "FETCHED",
                    "2": "REFUSAL"
                }
            },
            "witness": {
                "description": "Query witness ledger",
                "output_mode": "report",
                "exit_codes": {
                    "0": "OK"
                }
            }
        },
        "refusal_codes": {
            "E_EMPTY": "seal called with no artifacts",
            "E_IO": "Cannot read input, write output, or read pack directory",
            "E_DUPLICATE": "Member path collision during seal (including reserved paths)",
            "E_BAD_PACK": "Missing or invalid manifest.json for verify/diff/push"
        },
        "global_flags": ["--describe", "--schema", "--version", "--no-witness"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_manifest_has_required_fields() {
        let op = operator_json();
        assert_eq!(op["name"], "pack");
        assert_eq!(op["schema_version"], "operator.v0");
        assert_eq!(op["output_mode"], "mixed");
        assert!(op["version"].as_str().is_some());
    }

    #[test]
    fn operator_manifest_has_all_subcommands() {
        let op = operator_json();
        let subs = op["subcommands"].as_object().unwrap();
        assert!(subs.contains_key("seal"));
        assert!(subs.contains_key("verify"));
        assert!(subs.contains_key("diff"));
        assert!(subs.contains_key("push"));
        assert!(subs.contains_key("pull"));
        assert!(subs.contains_key("witness"));
    }

    #[test]
    fn operator_manifest_has_all_refusal_codes() {
        let op = operator_json();
        let codes = op["refusal_codes"].as_object().unwrap();
        assert!(codes.contains_key("E_EMPTY"));
        assert!(codes.contains_key("E_IO"));
        assert!(codes.contains_key("E_DUPLICATE"));
        assert!(codes.contains_key("E_BAD_PACK"));
    }

    #[test]
    fn operator_manifest_has_exit_codes() {
        let op = operator_json();
        let seal = &op["subcommands"]["seal"]["exit_codes"];
        assert_eq!(seal["0"], "PACK_CREATED");
        assert_eq!(seal["2"], "REFUSAL");

        let verify = &op["subcommands"]["verify"]["exit_codes"];
        assert_eq!(verify["0"], "OK");
        assert_eq!(verify["1"], "INVALID");
        assert_eq!(verify["2"], "REFUSAL");
    }

    #[test]
    fn operator_manifest_is_valid_json_string() {
        let op = operator_json();
        let json_str = serde_json::to_string_pretty(&op).unwrap();
        let _: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    }
}
