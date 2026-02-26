use serde_json::{json, Value};

/// Return the JSON Schema for pack.v0 manifest and verify output.
pub fn pack_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "pack.v0",
        "title": "pack.v0 manifest and verify schema",
        "definitions": {
            "manifest": {
                "type": "object",
                "required": ["version", "pack_id", "created", "tool_version", "members", "member_count"],
                "properties": {
                    "version": {
                        "type": "string",
                        "const": "pack.v0"
                    },
                    "pack_id": {
                        "type": "string",
                        "pattern": "^sha256:[a-f0-9]{64}$"
                    },
                    "created": {
                        "type": "string",
                        "format": "date-time"
                    },
                    "note": {
                        "type": ["string", "null"]
                    },
                    "tool_version": {
                        "type": "string"
                    },
                    "members": {
                        "type": "array",
                        "items": { "$ref": "#/definitions/member" }
                    },
                    "member_count": {
                        "type": "integer",
                        "minimum": 0
                    }
                },
                "additionalProperties": false
            },
            "member": {
                "type": "object",
                "required": ["path", "bytes_hash", "type"],
                "properties": {
                    "path": { "type": "string" },
                    "bytes_hash": {
                        "type": "string",
                        "pattern": "^sha256:[a-f0-9]{64}$"
                    },
                    "type": {
                        "type": "string",
                        "enum": ["lockfile", "report", "artifact", "rules", "pack", "profile", "registry", "other"]
                    },
                    "artifact_version": {
                        "type": ["string", "null"]
                    }
                },
                "additionalProperties": false
            },
            "verify_report": {
                "type": "object",
                "required": ["version", "outcome", "checks", "invalid"],
                "properties": {
                    "version": {
                        "type": "string",
                        "const": "pack.verify.v0"
                    },
                    "outcome": {
                        "type": "string",
                        "enum": ["OK", "INVALID", "REFUSAL"]
                    },
                    "pack_id": {
                        "type": ["string", "null"]
                    },
                    "checks": { "$ref": "#/definitions/verify_checks" },
                    "invalid": {
                        "type": "array",
                        "items": { "$ref": "#/definitions/invalid_finding" }
                    },
                    "refusal": {}
                },
                "additionalProperties": false
            },
            "verify_checks": {
                "type": "object",
                "required": ["manifest_parse", "member_count", "member_paths", "extra_members", "member_hashes", "pack_id", "schema_validation"],
                "properties": {
                    "manifest_parse": { "type": "boolean" },
                    "member_count": { "type": "boolean" },
                    "member_paths": { "type": "boolean" },
                    "extra_members": { "type": "boolean" },
                    "member_hashes": { "type": "boolean" },
                    "pack_id": { "type": "boolean" },
                    "schema_validation": {
                        "type": "string",
                        "enum": ["pass", "fail", "skipped"]
                    }
                },
                "additionalProperties": false
            },
            "invalid_finding": {
                "type": "object",
                "required": ["code"],
                "properties": {
                    "code": {
                        "type": "string",
                        "enum": [
                            "MISSING_MEMBER",
                            "HASH_MISMATCH",
                            "PACK_ID_MISMATCH",
                            "DUPLICATE_MEMBER_PATH",
                            "RESERVED_MEMBER_PATH",
                            "UNSAFE_MEMBER_PATH",
                            "NON_REGULAR_MEMBER",
                            "EXTRA_MEMBER",
                            "MEMBER_COUNT_MISMATCH"
                        ]
                    },
                    "path": { "type": "string" },
                    "expected": { "type": "string" },
                    "actual": { "type": "string" }
                },
                "additionalProperties": false
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_required_definitions() {
        let s = pack_schema();
        let defs = s["definitions"].as_object().unwrap();
        assert!(defs.contains_key("manifest"));
        assert!(defs.contains_key("member"));
        assert!(defs.contains_key("verify_report"));
        assert!(defs.contains_key("verify_checks"));
        assert!(defs.contains_key("invalid_finding"));
    }

    #[test]
    fn manifest_definition_has_required_fields() {
        let s = pack_schema();
        let required = s["definitions"]["manifest"]["required"].as_array().unwrap();
        let names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(names.contains(&"version"));
        assert!(names.contains(&"pack_id"));
        assert!(names.contains(&"created"));
        assert!(names.contains(&"tool_version"));
        assert!(names.contains(&"members"));
        assert!(names.contains(&"member_count"));
    }

    #[test]
    fn schema_is_valid_json() {
        let s = pack_schema();
        let json_str = serde_json::to_string_pretty(&s).unwrap();
        let _: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    }

    #[test]
    fn schema_has_id_and_title() {
        let s = pack_schema();
        assert_eq!(s["$id"], "pack.v0");
        assert!(s["title"].as_str().is_some());
    }
}
