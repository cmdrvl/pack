# AGENTS.md — pack

Guidelines for AI coding agents working in this repository.

## RULE 0 - USER OVERRIDE

If the user gives a direct instruction, follow it even if this file suggests otherwise.

## RULE 1 - NO FILE DELETION

Do not delete files or directories without explicit user approval in the same thread.

## Irreversible Actions (forbidden without explicit approval)

Never run destructive operations unless the user explicitly authorizes the exact command:

- `git reset --hard`
- `git clean -fd`
- `rm -rf`
- broad checkout/revert commands

If unsure, stop and ask.

## Main-Only Execution (No Worktrees by Default)

For this repo, default to working on `main` in the primary working tree.

- Do not spawn or use `--worktrees` unless the user explicitly asks for worktree isolation.
- If worktrees are active from an older session, call that out before making further edits.

## Kickoff Checklist (mandatory)

Before writing any code:

1. **Read `AGENTS.md` fully.** You are reading it now.
2. **Read `README.md` fully.** Understand the pack contract, exit semantics, and v0.1 scope.
3. **Read `docs/plan.md`** for contract-level behavior and boundaries.
4. **Run `br ready`** to see unblocked beads sorted by priority.
5. **Claim one bead:**
   ```bash
   br update <id> --status in_progress
   br show <id>
   ```
6. **Start implementation immediately.** Do not ask for confirmation to begin.

After completing a bead:

1. Verify acceptance criteria are met.
2. Run quality checks: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test -- --test-threads=1`
3. Close the bead: `br update <id> --status closed`
4. Run `br ready` again and claim the next unblocked bead.

Avoid communication purgatory: if blocked, claim another ready bead and continue.

## Bead Discipline

- Keep status current (`in_progress`, `blocked`, `closed`).
- Close beads only when acceptance criteria are actually satisfied.
- Post concise start/finish coordination updates with bead ID.
- If waiting, always pick next unblocked ready bead.

## File Reservation Policy (strict)

When multiple agents work concurrently, file reservations prevent conflicts.

**Reserve only exact files you are actively editing.**

Allowed reservations:

- `Cargo.toml` — you are adding a dependency
- `src/seal/collect.rs` — you are implementing collection logic
- `src/verify/schema.rs` — you are adding schema validation
- `tests/seal_suite.rs` — you are writing seal integration tests
- `README.md` — you are updating documentation

Forbidden reservations:

- `src/**` — too broad, blocks other agents from all source files
- `src/seal/` — directory-level claim blocks the whole module
- `tests/**` — blocks all test files
- `**/*` — claims the entire repo

**Release reservations as soon as your edits are complete.** Do not hold files between beads.

## Concurrent Edit Protocol

**Expect concurrent local edits from other agents.** This is normal in multi-agent workflows.

Rules:

1. **Never assume a clean working tree.** Other agents may have uncommitted changes in files you don't own.
2. **Never use destructive commands to force a clean state:**
   - No `git checkout .`
   - No `git clean -fd`
   - No `git stash` on someone else's work
   - No `rm -rf` on directories you don't own
3. **If you encounter unexpected changes in a file you need to edit:**
   - Check if another agent has reserved it. If so, skip or wait.
   - If unreserved, make your edit surgically (targeted `Edit` tool, not full file rewrites).
4. **Resolve conflicts surgically.** If a merge conflict appears in a file you own, fix only the conflicting region and continue.
5. **Do not reformat or reorganize files you did not change.** Stick to your bead's scope.

## Toolchain and Quality Checks

Rust/Cargo only.

After substantive changes, run:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
```

Note: `--test-threads=1` is required because witness tests manipulate the `EPISTEMIC_WITNESS` env var and cannot run in parallel.

If tests do not exist yet for a touched module, add focused tests or clearly note the gap.

## pack Contract Guardrails

Follow `docs/plan.md` as source of truth.

Critical behavior to preserve:

- deterministic `pack_id` self-hash contract
- closed-set pack directory semantics
- refusal envelope semantics (`E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`)
- verify outcomes and exit mapping (`OK`, `INVALID`, `REFUSAL`)
- witness behavior (`--no-witness`, append semantics, non-fatal failures)
- schema validation outcomes (`pass`, `fail`, `skipped`)

Do not invent behavior outside the plan without explicit user approval.

## Output and CLI Semantics

- Keep outputs deterministic and stable.
- Respect command exit codes exactly as specified.
- `--describe` and `--schema` must short-circuit before normal input validation.

## Commit Hygiene

- Keep commits scoped to one logical bead when possible.
- Do not bundle unrelated refactors.
- Do not amend commits unless the user asks.
