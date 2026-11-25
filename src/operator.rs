use serde_json::Value;

const OPERATOR_JSON: &str = include_str!("../operator.json");

/// Return the compiled-in operator manifest for `--describe`.
pub fn operator_json() -> Value {
    serde_json::from_str(OPERATOR_JSON).expect("operator.json must be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subcommand<'a>(op: &'a Value, name: &str) -> &'a Value {
        op["commands"]
            .as_array()
            .expect("operator commands must be an array")
            .iter()
            .find(|subcommand| subcommand["name"] == name)
            .expect("missing operator subcommand")
    }

    fn seal_option<'a>(op: &'a Value, name: &str) -> &'a Value {
        subcommand(op, "seal")["options"]
            .as_array()
            .expect("seal options must be an array")
            .iter()
            .find(|option| option["name"] == name)
            .expect("missing seal option")
    }

    #[test]
    fn operator_manifest_has_required_fields() {
        let op = operator_json();
        assert_eq!(op["name"], "pack");
        assert_eq!(op["schema_version"], "operator.v1");
        assert_eq!(op["invocation"]["output_mode"], "mixed");
        assert_eq!(op["tool_version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn operator_manifest_has_all_subcommands() {
        let op = operator_json();
        for name in [
            "seal",
            "verify",
            "inspect",
            "diff",
            "archive",
            "witness",
            "capabilities",
            "robot-docs",
            "doctor",
        ] {
            subcommand(&op, name);
        }
    }

    #[test]
    fn operator_manifest_has_all_refusal_codes() {
        let op = operator_json();
        let codes: Vec<&str> = op["refusals"]
            .as_array()
            .expect("operator refusals must be an array")
            .iter()
            .filter_map(|refusal| refusal["code"].as_str())
            .collect();
        assert!(codes.contains(&"E_EMPTY"));
        assert!(codes.contains(&"E_IO"));
        assert!(codes.contains(&"E_DUPLICATE"));
        assert!(codes.contains(&"E_BAD_PACK"));
    }

    #[test]
    fn operator_manifest_has_exit_codes() {
        let op = operator_json();

        let exit_codes = op["exit_codes"]
            .as_object()
            .expect("operator exit_codes must be an object");
        assert_eq!(exit_codes.len(), 3, "pack must declare the v1 exit trinity");
        assert_eq!(
            exit_codes["0"]["meaning"],
            "successful seal, valid verify, no-change diff, or healthy doctor"
        );
        assert_eq!(exit_codes["0"]["domain"], "positive");
        assert_eq!(
            exit_codes["1"]["meaning"],
            "invalid verify, changed diff, or unhealthy doctor"
        );
        assert_eq!(exit_codes["1"]["domain"], "negative");
        assert_eq!(exit_codes["2"]["meaning"], "refusal or command-line error");
        assert_eq!(exit_codes["2"]["domain"], "error");
        for (code, entry) in exit_codes {
            assert!(
                entry["meaning"]
                    .as_str()
                    .is_some_and(|meaning| !meaning.is_empty()),
                "exit code {code} must declare a non-empty v1 meaning"
            );
            assert!(
                entry["domain"]
                    .as_str()
                    .is_some_and(|domain| !domain.is_empty()),
                "exit code {code} must declare a non-empty v1 domain"
            );
        }

        let doctor = subcommand(&op, "doctor");
        assert_eq!(doctor["status"], "implemented");
        assert_eq!(doctor["read_only"], true);
        assert_eq!(doctor["witness"], "not_recorded");
        assert_eq!(doctor["fix_mode"], "not_available");

        let capabilities = subcommand(&op, "capabilities");
        assert_eq!(capabilities["read_only"], true);
        assert_eq!(capabilities["commands"][0], "pack capabilities --json");

        let robot_docs = subcommand(&op, "robot-docs");
        assert_eq!(robot_docs["read_only"], true);
        assert_eq!(robot_docs["commands"][0], "pack robot-docs guide");
    }

    #[test]
    fn operator_manifest_documents_seal_outcome_option() {
        let op = operator_json();
        let outcome = seal_option(&op, "outcome");
        assert_eq!(outcome["flag"], "--outcome");
    }

    #[test]
    fn operator_manifest_documents_profile_identity_fields() {
        let op = operator_json();
        assert_eq!(op["capabilities"]["profile_aware"], true);
        assert_eq!(
            op["capabilities"]["profile_identity"]["frozen_predicate"],
            "status: frozen plus profile_sha256"
        );
        let fields: Vec<&str> = op["manifest_member_fields"]
            .as_array()
            .expect("manifest_member_fields must be an array")
            .iter()
            .filter_map(|field| field.as_str())
            .collect();
        assert!(fields.contains(&"profile_frozen"));
        assert!(fields.contains(&"profile_sha256"));
        assert!(fields.contains(&"column_registry_hash"));
    }

    #[test]
    fn operator_manifest_has_no_transport_env_contract() {
        let op = operator_json();
        assert!(op["capabilities"].get("transport").is_none());
    }

    #[test]
    fn compiled_operator_matches_checked_in_operator_json() {
        let checked_in: Value =
            serde_json::from_str(OPERATOR_JSON).expect("operator.json must parse");
        assert_eq!(operator_json(), checked_in);
    }

    #[test]
    fn operator_manifest_is_valid_json_string() {
        let op = operator_json();
        let json_str = serde_json::to_string_pretty(&op).unwrap();
        let _: Value = serde_json::from_str(&json_str).unwrap();
    }
}
