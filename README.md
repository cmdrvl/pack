# pack

Seal lockfiles, reports, rules, and registry artifacts into one immutable, self-verifiable evidence pack.

`pack` is the deterministic answer to:

- What was known?
- How was it established?

A pack is both:

- A directory you can inspect
- A hash you can trust

## Status

`pack` is currently being implemented from the v0.1 plan in [`docs/plan.md`](docs/plan.md).
This README describes the target contract and operator behavior.

## TL;DR

Spine tools produce deterministic artifacts, but evidence is often scattered.
`pack` binds those artifacts into one content-addressed envelope.

Core outcomes:

- `pack seal`: creates a deterministic pack directory
- `pack verify`: proves integrity or reports deterministic invalid findings
- `pack witness`: queries witness history for pack operations

## Why pack exists

Without `pack`:

- artifacts live in scattered paths
- no single manifest binds them
- no single content ID for the full evidence set
- verification across the whole bundle is manual and brittle

With `pack`:

- members are copied byte-for-byte into one closed-set directory
- `manifest.json` binds all members and metadata
- `pack_id` is deterministic and self-verifiable
- verification has explicit exit semantics (`OK`, `INVALID`, `REFUSAL`)

## Scope v0.1

### Ship in v0.1

- `pack seal`
- `pack verify`
- `pack witness <query|last|count>`
- deterministic `pack_id` self-hash contract
- refusal system (`E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`)
- global flags: `--describe`, `--schema`, `--version`, `--no-witness`

### Deferred in v0.1

- `pack diff`
- `pack push`
- `pack pull`

## Quickstart (target flow)

```bash
# Produce upstream deterministic artifacts
shape nov.csv dec.csv --key loan_id --json > shape.report.json
rvl nov.csv dec.csv --key loan_id --json > rvl.report.json
verify dec.csv --rules rules.json --json > verify.report.json

# Seal into one deterministic evidence pack
pack seal nov.lock.json dec.lock.json shape.report.json rvl.report.json verify.report.json \
  --note "Nov→Dec 2025 reconciliation" \
  --output evidence/2025-12/

# Verify the sealed pack integrity
pack verify evidence/2025-12/

# Query witness ledger
pack witness last
```

## CLI surface

```text
pack <COMMAND> [OPTIONS]
```

### Commands

```text
seal <ARTIFACT>...        Seal artifacts into a pack directory
verify <PACK_DIR>         Verify pack integrity
witness <query|last|count>

diff <A> <B>              Deferred in v0.1
push <PACK_DIR>           Deferred in v0.1
pull <PACK_ID> --out DIR  Deferred in v0.1
```

### Global flags

- `--describe`: print compiled `operator.json`, exit 0
- `--schema`: print `pack.v0` schema, exit 0
- `--version`: print version, exit 0
- `--no-witness`: suppress witness writes

## Exit semantics

- `seal`: `0` (`PACK_CREATED`) or `2` (`REFUSAL`)
- `verify`: `0` (`OK`), `1` (`INVALID`), or `2` (`REFUSAL`)
- `diff` (deferred): `0` (`NO_CHANGES`), `1` (`CHANGES`), `2` (`REFUSAL`)
- `push`/`pull` (deferred): `0` success, `2` refusal

## Pack directory contract

`seal` output is a closed-set directory:

```text
pack/<pack_id>/
├── manifest.json
└── <member files...>
```

Rules:

- `manifest.json` must exist
- `manifest.json` is reserved and cannot be a member path
- member paths must be safe relative paths (no absolute, no `..`)
- only declared members plus `manifest.json` are allowed
- `member_count` must match declared members and actual files

## Manifest contract (`pack.v0`)

Key fields:

- `version`: `pack.v0`
- `pack_id`: `sha256:<hex>`
- `created`: RFC3339 UTC
- `note`: optional
- `tool_version`
- `members[]`: path, bytes hash, detected type, optional artifact version
- `member_count`

## Deterministic `pack_id`

`pack_id` is computed as:

1. construct manifest with `pack_id: ""`
2. canonicalize JSON deterministically
3. SHA256 hash canonical bytes
4. set `pack_id` to `sha256:<hex>`

Any change in manifest content changes `pack_id`.

## Verify checks

`pack verify` validates:

1. manifest exists and parses as `pack.v0`
2. `member_count` is correct
3. member paths are safe, unique, non-reserved
4. each member exists as a regular non-symlink file
5. no extra undeclared files exist
6. member hashes match
7. recomputed `pack_id` matches
8. known member schemas validate when local schema exists

Outcomes:

- `OK`: all checks pass
- `INVALID`: integrity/schema failures
- `REFUSAL`: unreadable/invalid manifest or unrecoverable IO

## Refusal codes

- `E_EMPTY`: no artifacts provided to `seal`
- `E_IO`: read/write/path IO failure
- `E_DUPLICATE`: member path collision (including reserved path)
- `E_BAD_PACK`: invalid or unreadable manifest for verify/diff/push

## Witness integration

- `seal` and `verify` append witness records by default
- `--no-witness` suppresses witness append
- witness append failure does not change domain exit semantics

## Installation

While `pack` is in active buildout, use source builds:

```bash
cargo build
cargo run -- --help
```

Release packaging and distribution are tracked in the plan/beads backlog.
