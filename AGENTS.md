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

## Kickoff Loop (mandatory)

Before coding:

1. Read `AGENTS.md` and `README.md` fully.
2. Read `docs/plan.md` for contract-level behavior and boundaries.
3. Run `br ready`.
4. Claim one unblocked bead:
   - `br update <id> --status in_progress`
   - `br show <id>`
5. Start implementation immediately.

Avoid communication purgatory: if blocked, claim another ready bead and continue.

## Bead Discipline

- Keep status current (`in_progress`, `blocked`, `closed`).
- Close beads only when acceptance criteria are actually satisfied.
- Post concise start/finish coordination updates with bead ID.
- If waiting, always pick next unblocked ready bead.

## File Reservation Policy (strict)

Reserve only exact files you are actively editing.

Allowed examples:

- `Cargo.toml`
- `src/main.rs`
- `src/seal/collect.rs`
- `README.md`
- `AGENTS.md`

Forbidden examples:

- `src/**`
- `docs/**`
- `**/*`
- full-directory claims

Release reservations as soon as edits are done.

## Parallel Collaboration Safety

- Assume concurrent edits are normal.
- Do not panic on unrelated local changes from other agents.
- Never use destructive cleanup to force a clean tree.
- Resolve conflicts surgically and continue momentum.

## Toolchain and Quality Checks

Rust/Cargo only.

After substantive changes, run:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

If tests do not exist yet for a touched module, add focused tests or clearly note the gap.

## pack Contract Guardrails

Follow `docs/plan.md` as source of truth.

Critical behavior to preserve:

- deterministic `pack_id` self-hash contract
- closed-set pack directory semantics
- refusal envelope semantics (`E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`)
- verify outcomes and exit mapping (`OK`, `INVALID`, `REFUSAL`)
- witness behavior (`--no-witness`, append semantics)

Do not invent behavior outside the plan without explicit user approval.

## Output and CLI Semantics

- Keep outputs deterministic and stable.
- Respect command exit codes exactly as specified.
- `--describe` and `--schema` must short-circuit before normal input validation.

## Commit Hygiene

- Keep commits scoped to one logical bead when possible.
- Do not bundle unrelated refactors.
- Do not amend commits unless the user asks.

