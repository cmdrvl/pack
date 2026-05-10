# pack conformance matrix

This matrix maps the behavioral contract in `AGENTS.md` and `docs/plan.md` to
test evidence. Requirement IDs are stable handles for CI failures and future
review work.

| Requirement ID | Contract | Evidence |
|---|---|---|
| PCK-MAN-001 | `pack_id` is the SHA-256 self-hash of the canonical manifest with `pack_id` empty during hashing. | `tests/seal_suite.rs::pack_id_self_hash_contract_holds`; `src/seal/manifest.rs::recompute_matches_finalized` |
| PCK-CREATED-001 | `seal` can produce reproducible `created` timestamps via `--created` or `SOURCE_DATE_EPOCH`, with `--created` taking precedence. | `tests/seal_suite.rs::seal_created_flag_makes_manifest_reproducible`; `tests/seal_suite.rs::seal_uses_source_date_epoch_when_created_flag_absent`; `src/seal/command.rs::created_flag_normalizes_rfc3339_to_utc` |
| PCK-DIR-001 | Pack directories are closed sets: only declared members plus `manifest.json` are allowed. | `tests/verify_suite.rs::extra_member_is_invalid`; `tests/verify_suite.rs::missing_member_is_invalid` |
| PCK-PATH-001 | Manifest member paths must be unique, safe relative paths and must not be `manifest.json`. | `tests/verify_suite.rs::unsafe_member_path_is_invalid`; `tests/verify_suite.rs::duplicate_member_path_is_invalid`; `tests/verify_suite.rs::reserved_member_path_is_invalid` |
| PCK-FILE-001 | Members must resolve to regular non-symlink files under the pack root. | `tests/verify_suite.rs::symlink_member_is_invalid`; `src/seal/collect.rs::symlink_refuses_with_e_io` |
| PCK-REF-001 | Refusals use the structured `pack.v0` envelope and preserve the refusal code/message contract. | `tests/refusal_suite.rs::seal_refusal_envelope_is_deterministic`; `src/refusal/envelope.rs::envelope_has_correct_shape` |
| PCK-VERIFY-001 | `verify` returns `INVALID` for parsed packs with integrity findings and `REFUSAL` for unreadable or malformed manifests. | `tests/verify_suite.rs::missing_member_is_invalid`; `tests/verify_suite.rs::malformed_manifest_is_refusal` |
| PCK-SCHEMA-001 | Known artifact types are schema-validated and report `pass`, `fail`, or `skipped`. | `tests/schema_validation.rs::valid_pack_schema_pass`; `tests/schema_validation.rs::wrong_version_schema_fail`; `tests/schema_validation.rs::other_only_pack_schema_skipped` |
| PCK-TYPE-001 | Member type detection is deterministic for known versions and registry/profile inputs. | `tests/seal_suite.rs::seal_type_detection_matches_expectations`; `src/detect/member_type.rs::detects_registry_by_path` |
| PCK-WIT-001 | Witness append failures never mutate domain outcome semantics. | `tests/witness_suite.rs::witness_failure_preserves_seal_domain_outcome`; `tests/witness_suite.rs::witness_failure_preserves_verify_domain_outcome`; `tests/witness_suite.rs::witness_failure_preserves_diff_domain_outcome` |
| PCK-INSPECT-001 | `inspect` reports deterministic metadata without claiming integrity and does not append witness records. | `src/inspect.rs::inspect_valid_pack_json_is_deterministic_metadata`; `src/inspect.rs::inspect_human_output_avoids_integrity_claim`; `src/inspect.rs::inspect_malformed_manifest_refuses`; `tests/witness_suite.rs::inspect_does_not_record_witness` |
| PCK-DIFF-001 | `diff` returns `NO_CHANGES` with exit 0 and `CHANGES` with exit 1. | `src/diff/command.rs::identical_packs_exit_0`; `src/diff/command.rs::different_packs_exit_1` |
| PCK-DESC-001 | `--describe` and `--schema` short-circuit before normal command validation. | `tests/cli_scaffold.rs::describe_short_circuits_before_validation`; `tests/cli_scaffold.rs::schema_short_circuits_before_validation` |
| PCK-OP-001 | `--describe` is byte-source-equivalent to the checked-in operator contract after JSON parsing. | `tests/cli_scaffold.rs::describe_matches_checked_in_operator_json`; `src/operator.rs::compiled_operator_matches_checked_in_operator_json` |
| PCK-PERF-001 | Large-pack performance measurement covers many-small and few-large scenarios, seal/verify/diff, determinism, throughput, and best-effort memory. | `tests/perf_baseline.rs::perf_report_shape_is_stable`; `tests/perf_baseline.rs::large_pack_performance_baseline` |
| PCK-PAR-001 | Parallel seal/verify hashing preserves deterministic result ordering and can be forced to one worker. | `src/seal/copy.rs::parallel_copy_matches_single_thread_order_and_hashes`; `src/verify/checks.rs::parallel_hash_findings_match_single_thread_order`; `src/parallel.rs::worker_count_can_force_single_thread` |
| PCK-STAGE-001 | `seal` stages beside the final output, refuses non-empty outputs, and never recursively copies into final output after promotion failure. | `src/staging.rs::refuses_non_empty_output_without_mutating_it`; `src/seal/command.rs::seal_promotes_into_existing_empty_output_dir` |
| PCK-ARCH-001 | `archive` exports deterministic tar wrappers, imports only safe manifest-first archives, verifies before promotion, and does not append witness records. | `src/archive.rs::archive_export_is_deterministic_for_same_pack`; `src/archive.rs::archive_import_round_trips_to_valid_pack`; `src/archive.rs::archive_import_tamper_refuses_and_leaves_no_output_dir`; `src/archive.rs::archive_import_refuses_unsafe_member_path`; `tests/cli_scaffold.rs::archive_export_import_cli_round_trip`; `tests/witness_suite.rs::archive_does_not_record_witness` |

## Known Gaps

- There is no dedicated performance pass/fail threshold yet. The ignored
  `perf_baseline` harness freezes the measurement format before optimization
  beads set targets.
