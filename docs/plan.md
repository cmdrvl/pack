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
- No clean push/pull boundary for durable, catalog-anchored evidence storage

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

For humans and skills operating outside the full pipeline, `pack seal` is the standalone sealing entrypoint once those artifacts already exist on disk. `lock` remains the upstream step that creates lockfiles; it does not replace pack's closed-set bundle semantics.

Typical flow:

```bash
shape nov.csv dec.csv --key loan_id --json > shape.report.json
rvl nov.csv dec.csv --key loan_id --json > rvl.report.json
verify dec.csv --rules rules.json --json > verify.report.json

pack seal nov.lock.json dec.lock.json shape.report.json rvl.report.json verify.report.json \
  --note "Nov→Dec 2025 reconciliation" --output evidence/2025-12/
```

Full chain of custody remains local-first. Receipt registration into the
metadata catalog is a separate cmdrvl-cli concern (`cmdrvl context pack
emit-receipt`) and is not part of the pack binary.

---

## Tool category

`pack` is a **subcommand tool with mixed output modes**:

- `seal`: directory artifact
- `verify` / `inspect` / `diff`: report output (human default, `--json` optional)
- `archive`: status output (deterministic file wrapper)

---

## CLI (current)

```text
pack <COMMAND> [OPTIONS]
```

### Commands

The list below is the current interface. `seal`, `verify`, `inspect`, `diff`,
`archive`, and `witness` are implemented. Receipt registration and resolution
are not pack subcommands — they live in cmdrvl-cli as
`cmdrvl context pack emit-receipt` and `cmdrvl context pack pull-receipt`. The
pack binary is intentionally Python-free and catalog-free so it stays usable
for standalone consumers who never touch the CMD+RVL operating environment.

```text
Commands:
  seal <ARTIFACT>...     Seal artifacts into an evidence pack directory
  verify <PACK_DIR>      Verify pack integrity (members + pack_id)
  inspect <PACK_DIR>     Inspect pack metadata without verifying integrity
  diff <A> <B>           Deterministically diff two packs
  archive <export|import>  Export/import deterministic archive wrappers
  witness <query|last|count>  Query witness ledger
```

### Subcommand details

```text
pack seal <ARTIFACT>... [--output <DIR>] [--note <TEXT>] [--created <RFC3339>] [--outcome <TAG>]
  <ARTIFACT>...          Files/directories to include
  --output <DIR>         Output directory (default: ~/.cmdrvl/state/pack/<pack_id>/)
  --note <TEXT>          Optional annotation in manifest
  --created <RFC3339>    Reproducible created timestamp, normalized to UTC
  --outcome <TAG>        Optional canonical anchor (must begin with cmdrvl://)

pack verify <PACK_DIR> [--json]

pack inspect <PACK_DIR> [--json]
  (metadata only; does not verify hashes, closed-set membership, or pack_id)

pack diff <A> <B> [--json]

pack archive export <PACK_DIR> --out <FILE>
pack archive import <ARCHIVE> --out <DIR>
  (deterministic uncompressed tar wrapper; directory pack remains canonical)

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
- `pack inspect`: `0` METADATA, `2` REFUSAL
- `pack diff`: `0` NO_CHANGES, `1` CHANGES, `2` REFUSAL
- `pack archive`: `0` ARCHIVE_CREATED or ARCHIVE_IMPORTED, `2` REFUSAL

### Output modes

| Subcommand | Output mode | `--json` |
|---|---|---|
| `seal` | Directory artifact (`manifest.json` + copied members) | N/A |
| `verify` | Human report | Yes |
| `inspect` | Human metadata report | Yes |
| `diff` | Human report | Yes |
| `archive` | Status lines | N/A |
| `witness` | Human report | Yes |

---

## Pack directory contract (`seal` output)

Default output path:

```text
~/.cmdrvl/state/pack/<pack_id>/
```

Example:

```text
~/.cmdrvl/state/pack/sha256:abc.../
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
- `seal` stages under the final output parent and promotes with atomic rename on success; it does not recursively copy into the final directory after promotion failure.
- When `--output` is absent, first use copies a legacy local `./pack/` directory into `~/.cmdrvl/state/pack/` if the canonical directory does not already exist, and records the migration under `~/.cmdrvl/migrations/` and `~/.cmdrvl/notices/`.

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
| `created` | string | yes | ISO 8601 UTC timestamp; reproducible via `--created` or `SOURCE_DATE_EPOCH` |
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

### Created timestamp selection

`pack seal` resolves `created` deterministically when the operator supplies a
timestamp source:

1. `--created <RFC3339>` takes precedence and is normalized to UTC seconds.
2. If `--created` is absent, `SOURCE_DATE_EPOCH` is accepted as Unix seconds.
3. If neither is supplied, `created` uses the current UTC time.

Invalid `--created` or `SOURCE_DATE_EPOCH` values refuse with exit `2` and a
structured `E_IO` refusal envelope. `--created` still takes precedence even when
`SOURCE_DATE_EPOCH` is present.

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

## `inspect` contract

`pack inspect <PACK_DIR>` reads pack metadata for quick human and agent
orientation. It is read-only and does not verify integrity.

It reports:

- `pack_id`, `created`, `tool_version`, optional `note`, and `member_count`.
- Member type counts sorted by type.
- Up to 10 largest members by on-disk file size, with deterministic tie-breaks.
- Schema-validation eligibility based on local validator coverage.
- `integrity.verified=false` plus a `pack verify <PACK_DIR>` next command.

It refuses with `E_BAD_PACK` when `manifest.json` is missing, malformed, or not
`pack.v0`. Missing or unreadable member files do not make `inspect` an
integrity verifier; their sizes are reported as unavailable and `pack verify`
remains the integrity check.

`inspect` does not append witness records.

---

## `diff` contract

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

## Receipt registration (lives in cmdrvl-cli, not pack)

Receipt registration into the metadata catalog and the catalog-pointed object
store is **not a pack subcommand**. The pack binary stays self-sufficient,
Python-free, and catalog-free so external consumers can use seal / verify /
inspect / diff / archive / witness without any CMD+RVL operating environment.

Inside CMD+RVL, receipt registration is provided by cmdrvl-cli as two
commands that read a verified pack directory from disk:

- `cmdrvl context pack emit-receipt <PACK_DIR>` — uploads the deterministic
  archive to S3 and registers `receipt_pack`, `operator_command`, `file`,
  and three link types in the metadata catalog v2. Mirrors the DataBook
  emission skeleton in `databook_emitter.py`.
- `cmdrvl context pack pull-receipt <PACK_ID> --out <DIR>` — resolves the
  receipt through the catalog, fetches the archive, runs `pack archive
  import` semantics for atomic staging + verify, and materializes the pack
  directory.

The on-disk pack directory remains the canonical integrity root. `pack
verify` is purely offline. The catalog-side records add a deterministic
transport pointer, lineage to the seal command, and lineage to the canon
entities the pack serves — but none of that is on pack's critical path.

### Receipt model

A *receipt pack* is the catalog-anchored, content-addressed evidence bundle
that `pack` produces. The on-disk pack directory is still the canonical
integrity root. `pack verify` remains purely offline. The catalog records add
a deterministic transport pointer, lineage to the seal command, and lineage to
the canon entities the pack serves.

Catalog contract (registered separately as
`metadata-seeds/salt/receipt-pack-spine/`):

- `resource_type://receipt_pack/1` — the pack itself, keyed by `pack_id`.
- `resource_type://operator_command/2` — reused from the DataBook spine.
- `link_type://pack_emitted_by/1` — receipt_pack → operator_command.
- `link_type://pack_payload_file/1` — receipt_pack → file (the archive in S3).
- `link_type://pack_serves_canon_entity/1` — receipt_pack → canon entity
  (substantive anchor: outcome, surface, data product, etc.).

IRIs are derived deterministically from `pack_id`:

- Pack IRI: `cmdrvl://catalog/<catalog>/receipt/pack/<pack_id>`
- Seal command IRI: `cmdrvl://catalog/<catalog>/operator/command/<seal_command_id>`
- Payload URI: `s3://cmdrvl-receipt-packs/tenant=<catalog>/receipts/packs/<YYYY-MM-DD>/<pack_id>/pack.tar`

### Emitter behavior summary

The full emitter contract — stages, idempotency, anchor resolution, refusal
codes, environment variables — is documented alongside the catalog seed in
`cmdrvl-cli/metadata-seeds/salt/receipt-pack-spine/README.md`. What pack
itself promises:

- The on-disk pack directory is the only canonical input. Any verified pack
  can be re-emitted into the catalog from disk; the reverse is not true.
- The deterministic archive produced by `pack archive export` is the wire
  format. Same source pack always yields byte-identical archive bytes, so
  the S3 path (content-addressed by `pack_id`) is idempotent.
- `pack seal --outcome <tag>` (when present) stamps the outcome anchor into
  the manifest, so the anchor is content-addressed into `pack_id`. The tag
  must be a canonical IRI beginning with `cmdrvl://`. Receipts for
  outcome-scoped seals are anchored by construction.
- The `created` timestamp from the manifest is the seal time. The emitter
  records that as `created_at` on the receipt resource and a separate
  `emitted_at` for registration time, mirroring DataBook conventions.

What pack does not promise:

- Pack does not own catalog retries, timeouts, or backoff. Those live in
  cmdrvl-cli's metadata client and S3 helper, shared with DataBook emission.
- Pack does not validate anchors. The cmdrvl-cli emitter resolves them
  through the catalog and refuses if zero anchors resolve.

---

## `archive` contract

Archives are deterministic transport wrappers around canonical pack directories.
They are not a new integrity root and do not replace `pack verify`.

`archive export`:

- Requires a valid pack directory and refuses if pack integrity checks fail.
- Writes an uncompressed ustar archive with `manifest.json` as the first entry.
- Serializes `manifest.json` from canonical manifest bytes, not from source-file whitespace.
- Writes members in manifest order, using fixed tar metadata (`mtime=0`, `uid=0`, `gid=0`, regular files only).
- Refuses if `--out` already exists.
- Produces byte-identical archive bytes for the same source pack.

`archive import`:

- Requires `manifest.json` to be the first archive entry.
- Refuses unsafe paths, duplicate archive paths, unsupported tar entry types, and malformed archives.
- Extracts into same-parent staging under `--out`.
- Verifies closed-set pack semantics after extraction and before promotion.
- Refuses if `--out` exists and is non-empty.
- Leaves no final output directory on failed import, or leaves a pre-existing empty output directory unchanged.

Witness policy:

- `archive` does not append witness records; transport wrapping is intentionally outside the default operation ledger. Use `pack verify` after import when a witness-backed integrity check is required.

### Responsibility split: disk vs. catalog vs. object store

`pack` collapses the persistence story into one model: the metadata catalog is
authoritative for discovery and lineage; an object store referenced by a
catalog `data_location` carries bytes. There is no separate data-fabric layer.

| Layer | Responsibility | Required for `pack verify`? |
|---|---|---|
| **disk pack** (`manifest.json` + members) | canonical artifact, content-addressed, self-verifying | yes — always |
| **catalog** (metadata catalog v2) | authoritative receipt registry: receipt_pack resources, file entries, links to commands and canon entities, IRIs | no — pull resolves through catalog, but verify is offline |
| **object store** (S3 bucket pointed to by `data_location`) | durable archive bytes addressed by `pack_id` | no — only for cross-machine handoff |

Discipline:

- **Disk pack is source of truth.** `pack verify` runs offline against the
  pack alone. A verifier with no network and no catalog access still produces
  a correct verdict from `manifest.json` + member hashes.
- **Catalog is the receipt ledger.** `push` registers a `receipt_pack`
  resource keyed by `pack_id`, links it to the seal command, the archive
  file, and any substantive canon-entity anchor (outcome, surface, data
  product). The catalog is the discovery surface and the lineage record.
- **Object store carries bytes only.** The archive is a deterministic
  uncompressed tar produced by `pack archive export`. Path is content-
  addressed by `pack_id`, so emit-receipt is idempotent on the wire and
  pull-receipt is hash-checkable against the receipt's `payload_sha256`.
- **Time is first-class.** The manifest's `created` timestamp survives into
  `created_at` on the receipt; `emitted_at` records when the receipt was
  registered; the partition date in the S3 path matches `created_at`.
- **Substantive anchor required.** A receipt cannot be orphaned. Push refuses
  with `MISSING_ANCHOR` if no preexisting catalog node is referenced. The
  seal-time `--outcome <tag>` anchor is preferred because it is content-
  addressed into `pack_id`.
- **Catalog is the cache, disk is the canon.** If catalog and disk diverge,
  disk wins. Catalog records can be re-emitted from a verified disk pack;
  the reverse is not true.

Concrete example — `benchmark` gold sets:

1. Gold set is sealed via
   `pack seal --outcome cmdrvl://catalog/salt/canon/entity/outcome/bdc` →
   produces a pack with `pack_id` and the outcome tag stamped into the
   manifest.
2. `cmdrvl context pack emit-receipt evidence/<pack_id>/` validates, exports
   the deterministic archive, uploads to
   `s3://cmdrvl-receipt-packs/tenant=salt/receipts/packs/<date>/<pack_id>/pack.tar`,
   and registers `resource://receipt_pack/<pack_id>` with a
   `pack_serves_canon_entity` link to
   `cmdrvl://catalog/salt/canon/entity/outcome/bdc`.
3. `benchmark` queries catalog for receipt_packs serving
   `cmdrvl://catalog/salt/canon/entity/outcome/bdc`,
   selects one by `pack_id`, then runs
   `cmdrvl context pack pull-receipt <pack_id> --out evidence/recovered/` to
   fetch the archive — so downstream `pack verify` works offline against the
   run.

Step 1's anchor moves through the manifest into the catalog without operator
intervention. The receipt is discoverable, content-addressed, and lineage-
linked from the moment it is published. This is the same pattern as DataBook
emission: cmdrvl-cli emits receipts into catalog; consumers resolve through
catalog and pull bytes through the linked object store, with the on-disk pack
remaining canonical for offline verification.

---

## Refusal codes

| Code | Trigger | Next step |
|---|---|---|
| `E_EMPTY` | `seal` called with no artifacts | Provide files/directories to seal |
| `E_IO` | Cannot read input, write output, or read pack dir | Check paths/permissions |
| `E_DUPLICATE` | Member path collision during seal | Rename inputs or adjust source layout |
| `E_BAD_PACK` | Missing/invalid pack payload for verify/diff/archive | Recreate pack via `pack seal` |

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
- Path: `EPISTEMIC_WITNESS` or `~/.cmdrvl/state/witness/witness.jsonl`.
- First use without `EPISTEMIC_WITNESS` copies a legacy `~/.epistemic/witness.jsonl` ledger into the canonical path when the canonical file does not already exist, and records the migration under `~/.cmdrvl/migrations/` and `~/.cmdrvl/notices/`.
- Witness append failure never changes domain exit semantics.

Recording policy:

- Record for `seal`, `verify`, and `diff`.
- Do not record for `inspect`, `archive`, or `witness` query subcommands.
- Receipt registration witness records (post-seal emit/pull) are written by
  cmdrvl-cli and live alongside DataBook emission witness entries; they are
  not a pack binary concern.

Witness outcome mapping:

- `seal`: `PACK_CREATED` or `REFUSAL`
- `verify`: `OK`, `INVALID`, or `REFUSAL`
- `diff`: `NO_CHANGES`, `CHANGES`, or `REFUSAL`

---

## Witness-to-pack projection plan

Witness-to-pack projection is planned as a provenance-driven artifact selection
layer for `pack seal`. It must not become a second integrity model.

Primary unit of work:

- The common case is an NTM session, especially an agent swarm/tournament.
- One NTM session normally maps to one projected evidence pack.
- A session pack should preserve the artifacts used by the tournament and the
  witness slice that explains why those artifacts were selected.

Simple v1 assumptions:

- Projection starts from an explicit witness ledger and an explicit bounded
  selector: `--session <ID>` when witness records carry a session ID, otherwise
  `--since <RFC3339>` / `--until <RFC3339>` with optional `--tool` and
  `--outcome` filters.
- Only records that parse as `witness.v0` are eligible; malformed records are
  skipped in preview mode and refused in seal mode if selected.
- Only local filesystem paths recorded as witness inputs are packable in v1.
- Every selected artifact must still exist and must match the recorded hash when
  a witness hash is present.
- Duplicate projected member paths refuse by default. The operator must rename
  artifacts or supply an explicit path mapping in a later design.
- Projection must require a dry-run preview by default and an explicit
  confirmation flag before sealing.
- The final pack is created by the normal `seal` pipeline; projection only
  chooses inputs and may add a projection report artifact.

Non-goals:

- Do not recreate missing artifacts from witness records.
- Do not trust witness hashes over current bytes.
- Do not silently include an entire NTM working directory.
- Do not infer session boundaries from tmux panes unless NTM exposes stable
  session metadata.
- Do not include outputs from generic records until witness records have a
  first-class `outputs[]` artifact list or an equivalent stable convention.

Expected first CLI shape, subject to implementation review:

```text
pack witness project --session <NTM_SESSION_ID> --out <DIR> [--dry-run] [--yes]
pack witness project --since <RFC3339> --until <RFC3339> --out <DIR> [--dry-run] [--yes]
```

Preview output should include:

- selected witness record count
- selected artifact paths
- missing paths
- hash mismatches
- duplicate member path candidates
- the command that would seal the projected pack

Seal mode should:

- refuse unless the preview has no missing paths, hash mismatches, or duplicate
  member paths
- write a `projection.report.json` artifact describing the selector, selected
  records, skipped records, and final artifact list
- include a `witness.slice.jsonl` artifact with the selected witness records
- invoke the existing seal contract so `pack_id` and closed-set semantics remain
  unchanged

Future phases:

1. Design-only contract: document the semantics above and keep implementation
   deferred until the witness session/output fields are crisp.
2. Local dry-run projection: implement preview from explicit ledger path and
   time/tool/outcome filters using current `inputs[]` records only.
3. Confirmed local sealing: generate `projection.report.json`,
   `witness.slice.jsonl`, then call the existing seal path after explicit
   `--yes`.
4. NTM session integration: support `--session <ID>` once NTM records stable
   session IDs, tournament names, and swarm metadata in witness records.
5. Output artifact support: include record outputs once witness records expose a
   stable `outputs[]` artifact list with path/hash/bytes.
6. Recovery integrations: optionally resolve missing artifacts from data-fabric
   or previous packs, but only with explicit operator approval and hash checks.

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
     d. Resolve default output under ~/.cmdrvl/state/pack when --output is absent, migrating legacy ./pack/ first when needed
     e. Prepare same-parent staging dir (refuse if final output exists and non-empty)
     f. Copy members into staging dir
     g. Build member metadata + type detection
     h. Build manifest with pack_id=""
     i. Canonicalize full manifest + compute SHA256 pack_id
     j. Write manifest.json and atomically promote staging dir without recursive-copy fallback
     k. Exit 0

   verify:
     a. Read manifest.json (E_BAD_PACK/E_IO on failure)
     b. Validate manifest shape and member_count
     c. Validate member-path uniqueness and reserved-name rules
     d. For each member: safe path + regular file + hash match
     e. Detect unexpected extra files under pack root
     f. Recompute pack_id from canonical manifest (`pack_id=""` during hash)
     g. Validate known member schemas from local catalog (skip when unavailable)
     h. Exit 0 (OK) or 1 (INVALID) or 2 (REFUSAL)

   inspect:
     a. Read manifest.json (E_BAD_PACK on failure)
     b. Summarize metadata, type counts, largest members, and schema eligibility
     c. State that integrity is not verified and point to `pack verify`
     d. Exit 0 or 2; do not append witness

   diff:
     a. Read both manifests
     b. Compare member sets + hashes
     c. Exit 0/1/2

   archive export:
     a. Read and verify pack directory
     b. Write deterministic uncompressed tar to `--out`
     c. Exit 0 or 2; do not append witness

   archive import:
     a. Read archive and require manifest-first layout
     b. Extract safe regular-file entries under same-parent staging
     c. Verify extracted pack directory
     d. Promote to `--out`
     e. Exit 0 or 2; do not append witness

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
├── inspect.rs
├── archive.rs
├── diff/
│   ├── diff.rs
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
- subcommands: `seal`, `verify`, `inspect`, `diff`, `archive`, `witness`
- refusal map: `E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`
- exit semantics by subcommand (0/1/2 pattern)

---

## Testing requirements

The plan conformance matrix in [`docs/CONFORMANCE.md`](./CONFORMANCE.md)
maps core requirements to concrete test evidence and is checked by
`tests/conformance_matrix.rs`.

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
- inspect reports metadata without claiming integrity and does not append witness
- archive export/import round trips to a valid pack, produces deterministic bytes for fixed source packs, refuses tampered archives, refuses unsafe archive paths, and does not append witness
- witness query/last/count behavior on synthetic ledgers
- `--describe` / `--schema` precedence before input validation
- ignored large-pack performance baseline for many-small and few-large packs
- parallel seal/verify hashing preserves ordering and can be forced single-threaded
- archive export/import deterministic wrapper behavior

Implemented post-v0.1 test tracks:

- `diff` command behavior
- `inspect` command behavior
- `archive` export/import wrapper behavior
- large-pack performance report shape and ignored local baseline

---

## Scope: v0.1 (shipped baseline)

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

- compressed archive formats (`tar.zst`), signing (`sigstore`), attestations (`in-toto`)
- witness-to-pack projection implementation beyond the design contract

### Current post-v0.1 additions

- `pack diff`
- witness append for `diff`
- reproducible `pack seal --created <RFC3339>` and `SOURCE_DATE_EPOCH`
- ignored large-pack performance baseline (`tests/perf_baseline.rs`)
- parallel seal/verify hashing controlled by `PACK_THREADS`
- deterministic `pack archive export` / `pack archive import`
- witness-to-pack projection design with NTM-session-first future phasing

### Removed (post-v0.1, replaced by cmdrvl-cli)

- `pack push` and `pack pull` (and `src/network/`) — receipt registration and
  resolution moved to cmdrvl-cli as `cmdrvl context pack emit-receipt` and
  `cmdrvl context pack pull-receipt`. The pack binary stays self-sufficient
  for standalone consumers; CMD+RVL agents compose seal + emit-receipt at the
  operator level. See `cmdrvl-cli/metadata-seeds/salt/receipt-pack-spine/`
  for the catalog contract.

---

## Open questions

- What exact witness field should carry a stable NTM session ID?
- Should projected packs include all successful records from a session by
  default, or only records from selected tool families?
- Should the projection report be mandatory in every projected pack, or should
  operators be able to suppress it for byte-for-byte minimal packs?
