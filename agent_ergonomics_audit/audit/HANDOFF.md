# pack Agent Ergonomics Handoff

Completed pass 1 on 2026-06-10.

## Applied

- Added top-level `pack --robot-triage`.
- Added top-level `pack capabilities --json`.
- Added top-level `pack robot-docs guide`.
- Added safe `pack doctor --fix` refusal with exact alternatives.
- Updated `operator.json`, README, AGENTS.md, `docs/plan.md`, CI smoke, and release preflight.
- Hardened release Homebrew formula generation for missing checksums and Homebrew version inference.

## Validation

- Regression scripts R-001 through R-004 are included under `audit/regression_tests/`.
- `validate_pass.sh` should be run against this workspace after code tests.

## Notes

The skill preflight reported missing `flock` on macOS; this pass continued
single-agent. Seal, verify, inspect, diff, archive, witness, refusal envelope,
pack ID, and witness behavior were intentionally left unchanged.

## Deferred

- `bd-i2l`: Generalized typo recovery for common flag and subcommand misspellings.
