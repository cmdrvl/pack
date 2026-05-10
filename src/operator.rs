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
        op["subcommands"]
            .as_array()
            .expect("operator subcommands must be an array")
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
        assert_eq!(op["schema_version"], "operator.v0");
        assert_eq!(op["invocation"]["output_mode"], "mixed");
        assert_eq!(op["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn operator_manifest_has_all_subcommands() {
        let op = operator_json();
        for name in [
            "seal", "verify", "inspect", "diff", "archive", "witness", "doctor",
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

        let seal = &subcommand(&op, "seal")["exit_codes"];
        assert_eq!(seal["0"]["meaning"], "PACK_CREATED");
        assert_eq!(seal["2"]["meaning"], "REFUSAL");

        let verify = &subcommand(&op, "verify")["exit_codes"];
        assert_eq!(verify["0"]["meaning"], "OK");
        assert_eq!(verify["1"]["meaning"], "INVALID");
        assert_eq!(verify["2"]["meaning"], "REFUSAL");

        let diff = subcommand(&op, "diff");
        assert_eq!(diff["status"], "implemented");
        assert_eq!(diff["exit_codes"]["0"]["meaning"], "NO_CHANGES");
        assert_eq!(diff["exit_codes"]["1"]["meaning"], "CHANGES");

        let inspect = subcommand(&op, "inspect");
        assert_eq!(inspect["status"], "implemented");
        assert_eq!(inspect["witness"], "not_recorded");
        assert_eq!(inspect["exit_codes"]["0"]["meaning"], "METADATA");

        let archive = subcommand(&op, "archive");
        assert_eq!(archive["status"], "implemented");
        assert_eq!(archive["witness"], "not_recorded");
        assert_eq!(
            archive["exit_codes"]["0"]["meaning"],
            "ARCHIVE_CREATED or ARCHIVE_IMPORTED"
        );

        let doctor = subcommand(&op, "doctor");
        assert_eq!(doctor["status"], "implemented");
        assert_eq!(doctor["read_only"], true);
        assert_eq!(doctor["witness"], "not_recorded");
        assert_eq!(doctor["fix_mode"], "not_available");
    }

    #[test]
    fn operator_manifest_documents_seal_outcome_option() {
        let op = operator_json();
        let outcome = seal_option(&op, "outcome");
        assert_eq!(outcome["flag"], "--outcome");
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
