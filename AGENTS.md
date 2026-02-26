# AGENTS.md — pack

> Guidelines for AI coding agents working in this Rust codebase.

---

## pack — What This Project Does

`pack` seals lockfiles, reports, rules, and registry artifacts into one immutable, self-verifiable evidence pack with a deterministic content-addressed `pack_id`.

Pipeline position:

```
vacuum → hash → fingerprint → lock → pack
```

### Quick Reference

```bash
# Core workflow
pack seal nov.lock.json dec.lock.json rules.json --output evidence/2025-12/
pack verify evidence/2025-12/
pack diff evidence/2025-11/ evidence/2025-12/

# Quality gate
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test -- --test-threads=1
```

Note: `--test-threads=1` is required because witness tests manipulate the `EPISTEMIC_WITNESS` env var and cannot run in parallel.

### Source of Truth

- **Spec:** [`docs/plan.md`](./docs/plan.md) — behavior must follow this document.
- Do not invent behavior not present in the plan.

### Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry + exit code mapping |
| `src/lib.rs` | Central dispatch, CLI parsing, command routing |
| `src/cli/` | Clap argument parsing, exit codes |
| `src/seal/` | Seal pipeline: collect, collision, copy, finalize, manifest |
| `src/verify/` | Verify pipeline: checks, schema validation, report |
| `src/diff/` | Diff pipeline: compare manifests, report |
| `src/detect/` | Member type detection from content |
| `src/refusal/` | Refusal codes and envelope |
| `src/witness/` | Witness ledger append/query |
| `src/operator.rs` | `--describe` output |
| `src/schema.rs` | `--schema` output |

---

## RULE 0 — USER OVERRIDE

If the user gives a direct instruction, follow it even if it conflicts with defaults in this file.

---

## Output Contract (Critical)

`pack` has three commands with distinct exit semantics:

| Command | Exit 0 | Exit 1 | Exit 2 |
|---------|--------|--------|--------|
| `seal` | `PACK_CREATED` | — | `REFUSAL` |
| `verify` | `OK` | `INVALID` | `REFUSAL` |
| `diff` | `NO_CHANGES` | `CHANGES` | `REFUSAL` |

- `seal` outputs human text on success, refusal JSON envelope on failure.
- `verify` outputs human or `--json` report.
- `diff` outputs human or `--json` report.
- `--describe` and `--schema` short-circuit before normal input validation.

---

## Core Invariants (Do Not Break)

### 1. `pack_id` self-hash contract

- Construct manifest with `pack_id: ""`
- Serialize to canonical JSON (sorted keys, no whitespace)
- SHA-256 hash the canonical bytes
- Set `pack_id` to `sha256:<hex>`

### 2. Closed-set directory semantics

- Only declared members plus `manifest.json` are allowed.
- Member paths must be safe relative paths (no absolute, no `..`).
- `manifest.json` is reserved — cannot be a member path.
- `member_count` must match the actual members array length.

### 3. Refusal envelope semantics

- Refusal codes: `E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`.
- Refusals emit structured JSON on stdout, exit code 2.
- Envelope includes `version`, `outcome`, `refusal.code`, `refusal.message`.

### 4. Verify outcomes

- `OK` (exit 0): all integrity checks pass.
- `INVALID` (exit 1): one or more findings with codes like `MISSING_MEMBER`, `HASH_MISMATCH`, `PACK_ID_MISMATCH`, `EXTRA_MEMBER`.
- `REFUSAL` (exit 2): manifest unreadable, unparseable, or unsupported version.

### 5. Schema validation

- Known artifact types are validated against local schemas.
- Outcomes: `pass`, `fail`, `skipped`.
- Schema failures produce `SCHEMA_VIOLATION` findings in verify report.

### 6. Witness parity

Ambient witness semantics must match spine conventions:
- Append by default to `$EPISTEMIC_WITNESS` or `~/.epistemic/witness.jsonl`.
- `--no-witness` opt-out.
- Witness failures do not mutate domain outcome semantics (non-fatal).
- Witness query subcommands supported (`query`, `last`, `count`).

---

## Toolchain

- **Language:** Rust, Cargo only.
- **Edition:** 2021.
- **Dependencies:** clap 4, serde 1, serde_json 1, sha2 0.10, hex 0.4, chrono 0.4, tempfile 3.

---

## Quality Gate

Run after any substantive change:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
```

### Test Suites

| Suite | File | Tests |
|-------|------|-------|
| CLI scaffold | `tests/cli_scaffold.rs` | 14 |
| Seal contract | `tests/seal_suite.rs` | 12 |
| Verify contract | `tests/verify_suite.rs` | 17 |
| Refusal envelope | `tests/refusal_suite.rs` | 9 |
| Schema validation | `tests/schema_validation.rs` | 4 |
| Witness behavior | `tests/witness_suite.rs` | 15 |
| Unit tests | `src/**` | 112 |
| **Total** | | **183** |

---

## Git and Release

- **Primary branch:** `main`.
- Bump `Cargo.toml` semver appropriately on release.
- Release triggered by pushing `v*` tags.
- CI runs fmt, clippy, unit, integration, smoke, ci-success.
- Release builds 5 targets (linux x86/arm, macOS x86/arm, windows).

---

## Editing Rules

- **No file deletion** without explicit written user permission.
- **No destructive git commands** (`reset --hard`, `clean -fd`, `rm -rf`, force push) without explicit authorization.
- **No scripted mass edits** — make intentional, reviewable changes.
- **No file proliferation** — edit existing files; create new files only for real new functionality.
- **No surprise behavior** — do not invent behavior not in `docs/plan.md`.
- **No backwards-compatibility shims** unless explicitly requested.

---

## Main-Only Execution (No Worktrees by Default)

For this repo, default to working on `main` in the primary working tree.

- Do not spawn or use `--worktrees` unless the user explicitly asks for worktree isolation.
- If worktrees are active from an older session, call that out before making further edits.

---

## Beads (`br`) Workflow

Use Beads as source of truth for task state.

```bash
br ready              # Show unblocked ready work
br list --status=open # All open issues
br show <id>          # Full issue details
br update <id> --status=in_progress
br close <id> --reason "Completed"
br sync --flush-only  # Export to JSONL (no git ops)
```

### Kickoff Loop

1. Run `br ready` to see unblocked beads sorted by priority.
2. Claim one bead: `br update <id> --status in_progress`
3. Read the bead: `br show <id>`
4. Start implementation immediately — do not ask for confirmation.
5. Verify acceptance criteria are met.
6. Run quality gate.
7. Close: `br update <id> --status closed`
8. Run `br ready` again and claim the next unblocked bead.

Avoid communication purgatory: if blocked, claim another ready bead and continue.

---

## Multi-Agent Coordination

### File Reservation Policy (strict)

When multiple agents work concurrently, reserve only exact files you are actively editing.

Allowed: `src/seal/collect.rs`, `tests/seal_suite.rs`, `README.md`
Forbidden: `src/**`, `src/seal/`, `tests/**`

Release reservations as soon as your edits are complete.

### Concurrent Edit Protocol

Expect concurrent local edits from other agents.

1. Never assume a clean working tree.
2. Never use destructive commands to force a clean state.
3. If you encounter unexpected changes in a file you need, check if another agent has reserved it.
4. Resolve conflicts surgically — fix only the conflicting region.
5. Do not reformat or reorganize files you did not change.

### Agent Mail

When Agent Mail is available:
- Register identity in this project.
- Send start/finish updates per bead.
- Poll inbox regularly and acknowledge `ack_required` messages promptly.

---

## Session Completion

Before ending a session:

1. Run quality gate (`fmt` + `clippy` + `test`).
2. Confirm docs/spec alignment for behavior changes.
3. Commit with precise message.
4. Push `main`.
5. Summarize: what changed, what was validated, remaining risks.
