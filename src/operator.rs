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
            .unwrap_or_else(|| panic!("missing operator subcommand {name}"))
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
        for name in ["seal", "verify", "diff", "push", "pull", "witness"] {
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

        let push = subcommand(&op, "push");
        assert_eq!(push["status"], "implemented");
        assert_eq!(push["exit_codes"]["0"]["meaning"], "PUBLISHED");

        let pull = subcommand(&op, "pull");
        assert_eq!(pull["status"], "implemented");
        assert_eq!(pull["exit_codes"]["0"]["meaning"], "FETCHED");
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
