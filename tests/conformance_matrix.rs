struct Requirement {
    id: &'static str,
    evidence: &'static [Evidence],
}

struct Evidence {
    file: &'static str,
    test: &'static str,
}

const MATRIX: &str = include_str!("../docs/CONFORMANCE.md");

const REQUIREMENTS: &[Requirement] = &[
    Requirement {
        id: "PCK-MAN-001",
        evidence: &[
            Evidence {
                file: "tests/seal_suite.rs",
                test: "pack_id_self_hash_contract_holds",
            },
            Evidence {
                file: "src/seal/manifest.rs",
                test: "recompute_matches_finalized",
            },
        ],
    },
    Requirement {
        id: "PCK-CREATED-001",
        evidence: &[
            Evidence {
                file: "tests/seal_suite.rs",
                test: "seal_created_flag_makes_manifest_reproducible",
            },
            Evidence {
                file: "tests/seal_suite.rs",
                test: "seal_uses_source_date_epoch_when_created_flag_absent",
            },
            Evidence {
                file: "src/seal/command.rs",
                test: "created_flag_normalizes_rfc3339_to_utc",
            },
        ],
    },
    Requirement {
        id: "PCK-DIR-001",
        evidence: &[
            Evidence {
                file: "tests/verify_suite.rs",
                test: "extra_member_is_invalid",
            },
            Evidence {
                file: "tests/verify_suite.rs",
                test: "missing_member_is_invalid",
            },
        ],
    },
    Requirement {
        id: "PCK-PATH-001",
        evidence: &[
            Evidence {
                file: "tests/verify_suite.rs",
                test: "unsafe_member_path_is_invalid",
            },
            Evidence {
                file: "tests/verify_suite.rs",
                test: "duplicate_member_path_is_invalid",
            },
            Evidence {
                file: "tests/verify_suite.rs",
                test: "reserved_member_path_is_invalid",
            },
        ],
    },
    Requirement {
        id: "PCK-FILE-001",
        evidence: &[
            Evidence {
                file: "tests/verify_suite.rs",
                test: "symlink_member_is_invalid",
            },
            Evidence {
                file: "src/seal/collect.rs",
                test: "symlink_refuses_with_e_io",
            },
        ],
    },
    Requirement {
        id: "PCK-REF-001",
        evidence: &[
            Evidence {
                file: "tests/refusal_suite.rs",
                test: "seal_refusal_envelope_is_deterministic",
            },
            Evidence {
                file: "src/refusal/envelope.rs",
                test: "envelope_has_correct_shape",
            },
        ],
    },
    Requirement {
        id: "PCK-VERIFY-001",
        evidence: &[
            Evidence {
                file: "tests/verify_suite.rs",
                test: "missing_member_is_invalid",
            },
            Evidence {
                file: "tests/verify_suite.rs",
                test: "malformed_manifest_is_refusal",
            },
        ],
    },
    Requirement {
        id: "PCK-SCHEMA-001",
        evidence: &[
            Evidence {
                file: "tests/schema_validation.rs",
                test: "valid_pack_schema_pass",
            },
            Evidence {
                file: "tests/schema_validation.rs",
                test: "wrong_version_schema_fail",
            },
            Evidence {
                file: "tests/schema_validation.rs",
                test: "other_only_pack_schema_skipped",
            },
        ],
    },
    Requirement {
        id: "PCK-TYPE-001",
        evidence: &[
            Evidence {
                file: "tests/seal_suite.rs",
                test: "seal_type_detection_matches_expectations",
            },
            Evidence {
                file: "src/detect/member_type.rs",
                test: "detects_registry_by_path",
            },
        ],
    },
    Requirement {
        id: "PCK-WIT-001",
        evidence: &[
            Evidence {
                file: "tests/witness_suite.rs",
                test: "witness_failure_preserves_seal_domain_outcome",
            },
            Evidence {
                file: "tests/witness_suite.rs",
                test: "witness_failure_preserves_verify_domain_outcome",
            },
            Evidence {
                file: "tests/witness_suite.rs",
                test: "witness_failure_preserves_diff_domain_outcome",
            },
        ],
    },
    Requirement {
        id: "PCK-INSPECT-001",
        evidence: &[
            Evidence {
                file: "src/inspect.rs",
                test: "inspect_valid_pack_json_is_deterministic_metadata",
            },
            Evidence {
                file: "src/inspect.rs",
                test: "inspect_human_output_avoids_integrity_claim",
            },
            Evidence {
                file: "src/inspect.rs",
                test: "inspect_malformed_manifest_refuses",
            },
            Evidence {
                file: "tests/witness_suite.rs",
                test: "inspect_does_not_record_witness",
            },
        ],
    },
    Requirement {
        id: "PCK-DIFF-001",
        evidence: &[
            Evidence {
                file: "src/diff/command.rs",
                test: "identical_packs_exit_0",
            },
            Evidence {
                file: "src/diff/command.rs",
                test: "different_packs_exit_1",
            },
        ],
    },
    Requirement {
        id: "PCK-NET-001",
        evidence: &[
            Evidence {
                file: "tests/refusal_suite.rs",
                test: "push_missing_base_url_e_io",
            },
            Evidence {
                file: "tests/refusal_suite.rs",
                test: "pull_not_found_e_io",
            },
            Evidence {
                file: "src/network/push.rs",
                test: "invalid_pack_refuses_before_network_publish",
            },
            Evidence {
                file: "src/network/transport.rs",
                test: "retryable_server_failures_are_retried_for_idempotent_requests",
            },
            Evidence {
                file: "src/network/transport.rs",
                test: "non_retryable_server_failure_is_not_retried",
            },
            Evidence {
                file: "src/network/transport.rs",
                test: "successful_non_json_response_is_decode_error",
            },
        ],
    },
    Requirement {
        id: "PCK-DESC-001",
        evidence: &[
            Evidence {
                file: "tests/cli_scaffold.rs",
                test: "describe_short_circuits_before_validation",
            },
            Evidence {
                file: "tests/cli_scaffold.rs",
                test: "schema_short_circuits_before_validation",
            },
        ],
    },
    Requirement {
        id: "PCK-OP-001",
        evidence: &[
            Evidence {
                file: "tests/cli_scaffold.rs",
                test: "describe_matches_checked_in_operator_json",
            },
            Evidence {
                file: "src/operator.rs",
                test: "compiled_operator_matches_checked_in_operator_json",
            },
        ],
    },
    Requirement {
        id: "PCK-PERF-001",
        evidence: &[
            Evidence {
                file: "tests/perf_baseline.rs",
                test: "perf_report_shape_is_stable",
            },
            Evidence {
                file: "tests/perf_baseline.rs",
                test: "large_pack_performance_baseline",
            },
        ],
    },
    Requirement {
        id: "PCK-PAR-001",
        evidence: &[
            Evidence {
                file: "src/seal/copy.rs",
                test: "parallel_copy_matches_single_thread_order_and_hashes",
            },
            Evidence {
                file: "src/verify/checks.rs",
                test: "parallel_hash_findings_match_single_thread_order",
            },
            Evidence {
                file: "src/parallel.rs",
                test: "worker_count_can_force_single_thread",
            },
        ],
    },
    Requirement {
        id: "PCK-STAGE-001",
        evidence: &[
            Evidence {
                file: "src/staging.rs",
                test: "refuses_non_empty_output_without_mutating_it",
            },
            Evidence {
                file: "src/seal/command.rs",
                test: "seal_promotes_into_existing_empty_output_dir",
            },
            Evidence {
                file: "src/network/pull.rs",
                test: "pull_promotes_into_existing_empty_output_dir",
            },
            Evidence {
                file: "src/network/pull.rs",
                test: "pull_failure_leaves_existing_empty_output_dir_unchanged",
            },
        ],
    },
    Requirement {
        id: "PCK-ARCH-001",
        evidence: &[
            Evidence {
                file: "src/archive.rs",
                test: "archive_export_is_deterministic_for_same_pack",
            },
            Evidence {
                file: "src/archive.rs",
                test: "archive_import_round_trips_to_valid_pack",
            },
            Evidence {
                file: "src/archive.rs",
                test: "archive_import_tamper_refuses_and_leaves_no_output_dir",
            },
            Evidence {
                file: "src/archive.rs",
                test: "archive_import_refuses_unsafe_member_path",
            },
            Evidence {
                file: "tests/cli_scaffold.rs",
                test: "archive_export_import_cli_round_trip",
            },
            Evidence {
                file: "tests/witness_suite.rs",
                test: "archive_does_not_record_witness",
            },
        ],
    },
];

#[test]
fn conformance_matrix_has_unique_requirement_ids() {
    let mut ids: Vec<&str> = REQUIREMENTS
        .iter()
        .map(|requirement| requirement.id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        REQUIREMENTS.len(),
        "duplicate conformance requirement id in tests/conformance_matrix.rs"
    );
}

#[test]
fn conformance_matrix_documents_every_requirement() {
    for requirement in REQUIREMENTS {
        assert!(
            MATRIX.contains(requirement.id),
            "docs/CONFORMANCE.md is missing {}",
            requirement.id
        );
        assert!(
            !requirement.evidence.is_empty(),
            "{} has no test evidence",
            requirement.id
        );
    }
}

#[test]
fn conformance_evidence_points_to_real_tests() {
    for requirement in REQUIREMENTS {
        for evidence in requirement.evidence {
            let source = source_for(evidence.file);
            assert!(
                source.contains(&format!("fn {}", evidence.test)),
                "{} references missing test {}::{}",
                requirement.id,
                evidence.file,
                evidence.test
            );
            assert!(
                MATRIX.contains(evidence.test),
                "docs/CONFORMANCE.md does not mention evidence {}::{} for {}",
                evidence.file,
                evidence.test,
                requirement.id
            );
        }
    }
}

fn source_for(file: &str) -> &'static str {
    match file {
        "src/detect/member_type.rs" => include_str!("../src/detect/member_type.rs"),
        "src/diff/command.rs" => include_str!("../src/diff/command.rs"),
        "src/archive.rs" => include_str!("../src/archive.rs"),
        "src/inspect.rs" => include_str!("../src/inspect.rs"),
        "src/network/pull.rs" => include_str!("../src/network/pull.rs"),
        "src/network/push.rs" => include_str!("../src/network/push.rs"),
        "src/network/transport.rs" => include_str!("../src/network/transport.rs"),
        "src/operator.rs" => include_str!("../src/operator.rs"),
        "src/parallel.rs" => include_str!("../src/parallel.rs"),
        "src/refusal/envelope.rs" => include_str!("../src/refusal/envelope.rs"),
        "src/seal/collect.rs" => include_str!("../src/seal/collect.rs"),
        "src/seal/command.rs" => include_str!("../src/seal/command.rs"),
        "src/seal/copy.rs" => include_str!("../src/seal/copy.rs"),
        "src/seal/manifest.rs" => include_str!("../src/seal/manifest.rs"),
        "src/staging.rs" => include_str!("../src/staging.rs"),
        "src/verify/checks.rs" => include_str!("../src/verify/checks.rs"),
        "tests/cli_scaffold.rs" => include_str!("cli_scaffold.rs"),
        "tests/perf_baseline.rs" => include_str!("perf_baseline.rs"),
        "tests/refusal_suite.rs" => include_str!("refusal_suite.rs"),
        "tests/schema_validation.rs" => include_str!("schema_validation.rs"),
        "tests/seal_suite.rs" => include_str!("seal_suite.rs"),
        "tests/verify_suite.rs" => include_str!("verify_suite.rs"),
        "tests/witness_suite.rs" => include_str!("witness_suite.rs"),
        other => panic!("unknown conformance evidence file {other}"),
    }
}
