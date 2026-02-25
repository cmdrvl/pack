# pack — Evidence Sealing

## One-line promise

**Seal lockfiles, reports, rules, and registry artifacts into one immutable, self-verifiable evidence pack.**

A `pack` is the deterministic answer to: *what was known, and how was it established?*

Second promise: **Evidence is a directory you can inspect and a hash you can trust.**

---

## Problem (clearly understood)

Spine tools produce deterministic artifacts (`lock`, `shape`, `rvl`, `verify`, `compare`, `canon`, `assess`).
Without `pack`, evidence is fragmented:

- Artifacts live in scattered paths
- There is no single manifest binding them together
- No content-addressed identifier for the full evidence set
- No deterministic way to verify a package is intact
- No clean push/pull boundary for durable storage in data-fabric

`pack` replaces that with one deterministic, content-addressed artifact envelope.

---

## Non-goals (explicit)

`pack` is NOT:

- A scanner (`vacuum`)
- A hasher (`hash`)
- A recognizer (`fingerprint`)
- A lock builder (`lock`)
- A report generator (`shape` / `verify` / `compare` / `rvl`)
- A decision engine (`assess`)

It does not decide whether results are true.
It seals and verifies the chain of evidence produced by deterministic tools.

---

## Relationship to the spine

`pack` is an **artifact tool with subcommands**.
It consumes artifacts produced by the spine and emits either:

- a sealed directory artifact (`seal`), or
- report-style verdicts (`verify`, `diff`).

Typical flow:

```bash
shape nov.csv dec.csv --key loan_id --json > shape.report.json
rvl nov.csv dec.csv --key loan_id --json > rvl.report.json
verify dec.csv --rules rules.json --json > verify.report.json

pack seal nov.lock.json dec.lock.json shape.report.json rvl.report.json verify.report.json \
  --note "Nov→Dec 2025 reconciliation" --output evidence/2025-12/
```

Full chain of custody remains local-first; push/pull is optional.

---

## Tool category

`pack` is a **subcommand tool with mixed output modes**:

- `seal`: directory artifact
- `verify` / `diff`: report output (human default, `--json` optional)
- `push` / `pull`: status output (network wrappers; deferred in v0.1)

---

## CLI (v0.1 target)

```text
pack <COMMAND> [OPTIONS]
```

### Commands

The list below is the full interface roadmap. v0.1 ships the subset in [Scope: v0.1](#scope-v01-ship-this).

```text
Commands:
  seal <ARTIFACT>...     Seal artifacts into an evidence pack directory
  verify <PACK_DIR>      Verify pack integrity (members + pack_id)
  diff <A> <B>           Deterministically diff two packs (deferred in v0.1)
  push <PACK_DIR>        Publish a pack to data-fabric (deferred in v0.1)
  pull <PACK_ID>         Fetch a pack by ID from data-fabric (deferred in v0.1)
  witness <query|last|count>  Query witness ledger
```

### Subcommand details

```text
pack seal <ARTIFACT>... [--output <DIR>] [--note <TEXT>]
  <ARTIFACT>...          Files/directories to include
  --output <DIR>         Output directory (default: pack/<pack_id>/)
  --note <TEXT>          Optional annotation in manifest

pack verify <PACK_DIR> [--json]

pack diff <A> <B> [--json]
  (deferred in v0.1)

pack push <PACK_DIR>
  (deferred in v0.1; thin data-fabric wrapper)

pack pull <PACK_ID> --out <DIR>
  (deferred in v0.1; thin data-fabric wrapper)

pack witness query [filters] [--json]
pack witness last [--json]
pack witness count [filters] [--json]
```

### Common flags (all subcommands)

- `--describe`: Print `operator.json` to stdout and exit 0 (checked before input validation).
- `--schema`: Print JSON Schema for `pack.v0` and exit 0 (checked before input validation).
- `--version`: Print `pack <semver>` and exit 0.
- `--no-witness`: Suppress witness ledger recording.

### Exit codes

- `pack seal`: `0` PACK_CREATED, `2` REFUSAL
- `pack verify`: `0` OK, `1` INVALID, `2` REFUSAL
- `pack diff`: `0` NO_CHANGES, `1` CHANGES, `2` REFUSAL
- `pack push`: `0` PUBLISHED, `2` REFUSAL
- `pack pull`: `0` FETCHED, `2` REFUSAL

### Output modes

| Subcommand | Output mode | `--json` |
|---|---|---|
| `seal` | Directory artifact (`manifest.json` + copied members) | N/A |
| `verify` | Human report | Yes |
| `diff` | Human report (deferred v0.1) | Yes |
| `push`, `pull` | Status lines (deferred v0.1) | N/A |
| `witness` | Human report | Yes |

---

## Pack directory contract (`seal` output)

Default output path:

```text
pack/<pack_id>/
```

Example:

```text
pack/sha256:abc.../
├── manifest.json
├── nov.lock.json
├── dec.lock.json
├── shape.report.json
├── rvl.report.json
└── verify.report.json
```

Rules:

- `manifest.json` is always present.
- Every manifest member must exist as a file relative to pack root.
- `manifest.json` is reserved and must not appear in `members[].path`.
- Pack root is closed-set: only `manifest.json` plus declared member files are allowed.
- Member files are copied byte-for-byte from source inputs.
- `member_count` equals `len(members)` and equals files listed (excluding `manifest.json`).
- `seal` refuses if target output directory already exists and is non-empty.
- `seal` writes via a temp staging directory and atomically renames on success (no partial pack on failure).

---

## Manifest schema (`pack.v0`)

```json
{
  "version": "pack.v0",
  "pack_id": "sha256:...",
  "created": "2026-01-15T10:30:00Z",
  "note": "Q4 2025 loan tape reconciliation",
  "tool_version": "0.1.0",
  "members": [
    {
      "path": "nov.lock.json",
      "bytes_hash": "sha256:...",
      "type": "lockfile",
      "artifact_version": "lock.v0"
    },
    {
      "path": "rvl.report.json",
      "bytes_hash": "sha256:...",
      "type": "report",
      "artifact_version": "rvl.v0"
    }
  ],
  "member_count": 5
}
```

### Field definitions

| Field | Type | Required | Notes |
|---|---|---|---|
| `version` | string | yes | Always `"pack.v0"` |
| `pack_id` | string | yes | Self-hash (computed last from canonical manifest with `pack_id=""`) |
| `created` | string | yes | ISO 8601 UTC timestamp |
| `note` | string/null | no | Optional annotation |
| `tool_version` | string | yes | `pack` semver that created the pack |
| `members` | array | yes | Sorted member descriptors |
| `member_count` | int | yes | Equals number of `members` |
| `members[].path` | string | yes | Relative path within pack directory |
| `members[].bytes_hash` | string | yes | `sha256:<hex>` of member bytes |
| `members[].type` | string | yes | Auto-detected member type |
| `members[].artifact_version` | string/null | no | Parsed artifact `version` when available |

---

## `pack_id` integrity contract

`pack_id` is content-addressed and deterministic:

1. Build manifest with `pack_id: ""`.
2. Canonicalize JSON (stable key ordering, deterministic arrays/order).
3. Compute SHA256 over canonical bytes.
4. Set `pack_id` to `"sha256:<hex>"`.

Any change in manifest content (including note, created, tool_version, members, member order, or copied bytes) changes `pack_id`.

---

## Member collection and normalization

`pack seal` accepts file and directory arguments.

Collection rules:

- File argument: include as one member using basename as default member path.
- Directory argument: recursively include all files under that directory using `<dir_basename>/<relative_path>`.
- Traversal order is deterministic: bytewise ascending path order.
- Member paths are normalized to relative POSIX-style paths (`/` separators), never absolute, and never include `..` segments.
- Only regular files are admissible members; symlinks, sockets, devices, and FIFOs refuse with `E_IO`.

Collision rule:

- If two candidate members resolve to the same member `path`, refusal `E_DUPLICATE`.
- Reserved member path `manifest.json` also refuses with `E_DUPLICATE`.

Copy rule:

- Copy bytes exactly from source to destination member path.
- Compute `bytes_hash` from copied bytes (not metadata).

---

## Member type detection

`pack` infers member type from parseable content and version markers:

- `lock.v0` → `lockfile`
- `rvl.v0`, `shape.v0`, `verify.v0`, `compare.v0` → `report`
- `canon.v0`, `assess.v0` → `artifact`
- `verify.rules.v0` → `rules`
- `pack.v0` → `pack`
- YAML with `schema_version` + `profile_id` → `profile`
- Files from materialized registry artifacts (for example `registry.json` and registry tables) → `registry`
- Everything else → `other`

`artifact_version` is populated when a recognized version field exists.

---

## `verify` contract

`pack verify` validates integrity of an existing pack directory.

Checks:

1. `manifest.json` exists and parses.
2. Manifest is `pack.v0`.
3. `member_count == len(members)`.
4. `members[].path` values are unique and do not include reserved `manifest.json`.
5. Every `members[].path` is a safe relative path (no absolute/`..`) and resolves to a regular non-symlink file under pack root.
6. No extra files are present under pack root beyond `manifest.json` and declared member paths.
7. Re-hash each member and compare with `members[].bytes_hash`.
8. Recompute `pack_id` using the same canonical-manifest procedure (`pack_id=""` during hash) and compare with manifest `pack_id`.
9. Validate known JSON members against locally available schemas (no network fetch during verify).

If a known type has no local schema installed, verification records that check as skipped (not `INVALID`).

Outcomes:

- `OK` (exit 0): all checks passed.
- `INVALID` (exit 1): pack parsed, but one or more integrity/schema checks failed.
- `REFUSAL` (exit 2): unreadable/invalid manifest or unrecoverable IO error.

### `verify` JSON shape (when `--json`)

```json
{
  "version": "pack.verify.v0",
  "outcome": "OK | INVALID | REFUSAL",
  "pack_id": "sha256:...",
  "checks": {
    "manifest_parse": true,
    "member_count": true,
    "member_paths": true,
    "extra_members": true,
    "member_hashes": true,
    "pack_id": true,
    "schema_validation": "pass | fail | skipped"
  },
  "invalid": [],
  "refusal": null
}
```

For `INVALID`, `invalid` contains deterministic entries like:

- `{ "code": "MISSING_MEMBER", "path": "verify.report.json" }`
- `{ "code": "HASH_MISMATCH", "path": "rvl.report.json", "expected": "sha256:...", "actual": "sha256:..." }`
- `{ "code": "PACK_ID_MISMATCH", "expected": "sha256:...", "actual": "sha256:..." }`
- `{ "code": "DUPLICATE_MEMBER_PATH", "path": "rvl.report.json" }`
- `{ "code": "RESERVED_MEMBER_PATH", "path": "manifest.json" }`
- `{ "code": "UNSAFE_MEMBER_PATH", "path": "../outside.txt" }`
- `{ "code": "NON_REGULAR_MEMBER", "path": "linked.report.json" }`
- `{ "code": "EXTRA_MEMBER", "path": "tmp/debug.txt" }`

---

## `diff` contract (roadmap; deferred in v0.1)

`pack diff <A> <B>` compares manifests by member set and member hashes.

Core comparison:

- Added members
- Removed members
- Changed members (`same path`, different `bytes_hash`)

Optional enrichment for known reports:

- Surface high-level outcome shifts (e.g., `rvl: NO_REAL_CHANGE -> REAL_CHANGE`).

Exit semantics:

- `0` no differences
- `1` differences found
- `2` refusal

---

## `push` / `pull` contract (roadmap; deferred in v0.1)

Thin wrappers to data-fabric; no new domain logic.

`push`:

- Publish manifest + member metadata under `pack_id`.
- Idempotent for same `pack_id`.

`pull`:

- Fetch pack by `pack_id`.
- Materialize `manifest.json` + members under `--out`.

Failure mapping:

- Network / transport / not-found issues → refusal (exit 2).

---

## Refusal codes

| Code | Trigger | Next step |
|---|---|---|
| `E_EMPTY` | `seal` called with no artifacts | Provide files/directories to seal |
| `E_IO` | Cannot read input, write output, or read pack dir | Check paths/permissions |
| `E_DUPLICATE` | Member path collision during seal | Rename inputs or adjust source layout |
| `E_BAD_PACK` | Missing/invalid `manifest.json` for verify/diff/push | Recreate pack via `pack seal` |

### Refusal envelope

```json
{
  "version": "pack.v0",
  "outcome": "REFUSAL",
  "refusal": {
    "code": "E_DUPLICATE",
    "message": "Resolved member path collision",
    "detail": {
      "path": "nov.lock.json",
      "sources": ["/a/nov.lock.json", "/b/nov.lock.json"]
    },
    "next_command": null
  }
}
```

---

## Witness integration

`pack` follows the spine witness protocol:

- Default: append one `witness.v0` record per eligible invocation.
- Opt-out: `--no-witness`.
- Path: `EPISTEMIC_WITNESS` or `~/.epistemic/witness.jsonl`.
- Witness append failure never changes domain exit semantics.

Recording policy in v0.1 target:

- Record for `seal` and `verify`.
- Do not record for `witness` query subcommands.
- `diff` / `push` / `pull` record when implemented.

Witness outcome mapping:

- `seal`: `PACK_CREATED` or `REFUSAL`
- `verify`: `OK`, `INVALID`, or `REFUSAL`

---

## Execution flow

```text
1. Parse CLI args
2. If --describe: print operator.json, exit 0
3. If --schema: print pack schema, exit 0
4. If witness subcommand: dispatch query/last/count, exit
5. Dispatch command:

   seal:
     a. Resolve/collect artifacts (files + recursive dirs)
     b. Refuse E_EMPTY if none
     c. Resolve member paths + detect collisions (E_DUPLICATE)
     d. Prepare staging dir (refuse if final output exists and non-empty)
     e. Copy members into staging dir
     f. Build member metadata + type detection
     g. Build manifest with pack_id=""
     h. Canonicalize full manifest + compute SHA256 pack_id
     i. Write manifest.json and atomically promote staging dir
     j. Exit 0

   verify:
     a. Read manifest.json (E_BAD_PACK/E_IO on failure)
     b. Validate manifest shape and member_count
     c. Validate member-path uniqueness and reserved-name rules
     d. For each member: safe path + regular file + hash match
     e. Detect unexpected extra files under pack root
     f. Recompute pack_id from canonical manifest (`pack_id=""` during hash)
     g. Validate known member schemas from local catalog (skip when unavailable)
     h. Exit 0 (OK) or 1 (INVALID) or 2 (REFUSAL)

   diff (when implemented):
     a. Read both manifests
     b. Compare member sets + hashes
     c. Exit 0/1/2

   push/pull (when implemented):
     a. Transport call to data-fabric
     b. Exit 0 or 2

6. Append witness record (if applicable, if not --no-witness)
7. Exit
```

---

## Rust module sketch

```text
src/
├── cli/
│   ├── args.rs
│   ├── exit.rs
│   └── mod.rs
├── seal/
│   ├── collect.rs
│   ├── copy.rs
│   ├── manifest.rs
│   └── mod.rs
├── verify/
│   ├── verify.rs
│   └── mod.rs
├── diff/                # deferred in v0.1
│   ├── diff.rs
│   └── mod.rs
├── network/             # deferred in v0.1
│   ├── push.rs
│   ├── pull.rs
│   └── mod.rs
├── detect/
│   ├── member_type.rs
│   └── mod.rs
├── refusal/
│   ├── codes.rs
│   ├── payload.rs
│   └── mod.rs
├── witness/
│   ├── record.rs
│   ├── ledger.rs
│   ├── query.rs
│   └── mod.rs
├── output/
│   ├── human.rs
│   ├── json.rs
│   └── mod.rs
├── lib.rs
└── main.rs
```

---

## Operator manifest (`operator.json`)

`pack` must ship a compiled-in operator manifest for `--describe`.

Required highlights:

- `name: "pack"`
- `schema_version: "operator.v0"`
- `output_mode: "mixed"`
- subcommands: `seal`, `verify`, `diff`, `push`, `pull`, `witness`
- refusal map: `E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`
- exit semantics by subcommand (0/1/2 pattern)

---

## Testing requirements

### Fixtures

- `fixtures/artifacts/`:
  - sample lock/report/rules/profile files
  - nested registry directory fixture
  - duplicate-name fixture
- `fixtures/packs/`:
  - valid pack
  - missing-member pack
  - tampered-member pack
  - tampered-manifest pack

### Test categories

- `seal` creates deterministic manifest for identical inputs
- member ordering is deterministic (bytewise path order)
- `pack_id` is stable and self-verifiable
- `pack_id` changes when any manifest field changes (including metadata fields)
- duplicate path collision returns `E_DUPLICATE`
- non-regular input members (symlink/socket/device/FIFO) refuse with `E_IO`
- existing non-empty output dir refuses and leaves no partial pack behind
- verify flags unsafe manifest member paths (absolute or `..`) as `INVALID`
- verify flags duplicate member paths and reserved member path `manifest.json` as `INVALID`
- verify flags extra unexpected files under pack root as `INVALID`
- type detection mapping is deterministic for known versions
- `verify` returns:
  - `OK` on valid pack
  - `INVALID` on missing member/hash mismatch/pack_id mismatch/schema mismatch/extra member/non-regular member/duplicate or reserved member path
  - `REFUSAL` on unreadable or malformed manifest
- refusal envelope correctness for all refusal codes
- witness append/no-witness behavior
- witness query/last/count behavior on synthetic ledgers
- `--describe` / `--schema` precedence before input validation

Deferred test tracks:

- `diff` command behavior
- `push` / `pull` transport mapping

---

## Scope: v0.1 (ship this)

### Must have

- `pack seal`
- `pack verify`
- `pack witness <query|last|count>`
- deterministic `pack_id` self-hash contract
- member type detection + manifest contract (`pack.v0`)
- refusal system (`E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`)
- `--describe`, `--schema`, `--version`, `--no-witness`
- witness append for `seal` / `verify`

### Can defer

- `pack diff`
- `pack push` / `pack pull`
- archive formats (`tar.zst`), signing (`sigstore`), attestations (`in-toto`)
- witness-driven pack projection mode (`witness export` integration)

---

## Open questions

- Should `seal` support `--created <RFC3339>` (or `SOURCE_DATE_EPOCH`) to allow reproducible repacks across different run times? Not blocking v0.1.
