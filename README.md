# pack

![pack: content-addressing as the proof. A painterly dashboard showing five input artifacts entering a brass stamping mechanism labeled PACK SEAL, producing a content-addressed .tar archive named sha256:75757cac…88bf0. Two pack verify lines confirm the archive is identical and would fail on any tampered byte. A bottom strip shows two operators on different machines producing the same hash, captioned the name is the proof.](docs/images/pack.webp)

> *Name a bundle by what's inside it. Prove tampering by hashing it again.*

<div align="center">

[![CI](https://github.com/cmdrvl/pack/actions/workflows/ci.yml/badge.svg)](https://github.com/cmdrvl/pack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub release](https://img.shields.io/github/v/release/cmdrvl/pack)](https://github.com/cmdrvl/pack/releases)

**The pipeline produced the evidence. pack seals it shut.**

```bash
brew install cmdrvl/tap/pack
```

</div>

---

You've run the full pipeline: vacuum, hash, fingerprint, lock, shape, rvl. You have a lockfile, a shape report, an rvl report, and a mapping artifact. Four JSON files in a directory. If an auditor asks for the evidence — you zip them and email them? What if someone edits a file after you sent it? What if you need to prove, six months later, that nothing changed?

**pack seals artifacts into one immutable, content-addressed evidence pack with a single verifiable ID.** One command to seal. One command to verify, bit-for-bit. The `pack_id` is the SHA-256 of the canonical manifest — if any member is added, removed, or modified, the ID changes. No undeclared files allowed. No ambiguity about what's inside.

If you already have the artifacts you want to preserve, start with `pack seal`. That is the standalone sealing interface for humans and skills. `lock` still matters upstream when you need to generate a lockfile, but `lock` produces one artifact; `pack seal` is what turns the full evidence set into a durable bundle.

### What makes this different

- **Closed-set enforcement** — only declared members plus `manifest.json` are allowed in the pack directory. Extra files cause verification failure. Nothing sneaks in.
- **Content-addressed ID** — `pack_id` is a Merkle-root-like SHA-256 of the canonical manifest. Same artifacts and manifest metadata produce the same ID; pass `--created` for reproducible repacks. Any change — even to one byte of one member — produces a different ID.
- **Artifact type detection** — pack auto-classifies members as lockfiles, reports, profiles, or registries from their content. Frozen profile members carry their declared `profile_sha256` and `column_registry_hash` when present. Known JSON types are validated against local schemas during verification.
- **Diff between packs** — `pack diff evidence/nov/ evidence/dec/` shows exactly which members were added, removed, or changed between two evidence sets.

---

## Quick Example

### Standalone sealing path

If the artifacts already exist on disk, `pack seal` is the direct entrypoint:

```bash
# Seal artifacts into an evidence pack
$ pack seal nov.lock.json dec.lock.json shape.report.json rvl.report.json \
    --note "Nov→Dec 2025 reconciliation" \
    --output evidence/2025-12/
PACK_CREATED sha256:e78de23c97bc6b76...
evidence/2025-12/

# Verify the sealed pack
$ pack verify evidence/2025-12/
pack verify: OK
  pack_id: sha256:e78de23c97bc6b76...

# Diff two packs
$ pack diff evidence/2025-11/ evidence/2025-12/
pack diff: CHANGES
  a: sha256:a1b2c3d4...
  b: sha256:e78de23c...
  added: 1
    + rvl.report.json
  changed: 1
    ~ dec.lock.json
  unchanged: 2

# Check the witness ledger
$ pack witness last
2026-02-25T12:00:00.000Z verify OK -

# Full pipeline when you still need to produce the lockfile first
$ vacuum /data/dec | hashbytes | lock --dataset-id "dec" > dec.lock.json
$ pack seal dec.lock.json shape.report.json --output evidence/dec/
```

---

## Where pack Fits

`pack seal` is the recommended standalone sealing interface. In the full spine pipeline, `pack` is also the **final tool** — it seals everything into an immutable evidence set after upstream tools have produced their artifacts.

```
vacuum  →  hashbytes  →  fingerprint  →  lock  →  pack
(scan)    (hashbytes)    (template)     (pin)    (seal)
```

Upstream tools produce individual artifacts (lockfiles, reports). `pack seal` collects them into a single verifiable bundle with a content-addressed identifier.

---

## What pack Is Not

`pack` does not replace upstream pipeline tools.

| If you need... | Use |
|----------------|-----|
| Enumerate files in a directory | [`vacuum`](https://github.com/cmdrvl/vacuum) |
| Compute SHA-256/BLAKE3 hashes | [`hash`](https://github.com/cmdrvl/hash) |
| Match files against template definitions | [`fingerprint`](https://github.com/cmdrvl/fingerprint) |
| Pin artifacts into a self-hashed lockfile | [`lock`](https://github.com/cmdrvl/lock) |
| Check structural comparability of CSVs | [`shape`](https://github.com/cmdrvl/shape) |
| Explain numeric changes between CSVs | [`rvl`](https://github.com/cmdrvl/rvl) |

`lock` creates one pinned artifact. `pack seal` is the standalone bundling step that preserves the whole set.

`pack` only answers: **are these artifacts sealed, intact, and verifiable as a set?**

---

## The Three Outcomes

### seal

| Exit Code | Outcome | Meaning |
|-----------|---------|---------|
| `0` | `PACK_CREATED` | All artifacts sealed, `pack_id` computed, directory written |
| `2` | `REFUSAL` | Cannot seal — missing files, duplicates, or I/O failure |

### verify

| Exit Code | Outcome | Meaning |
|-----------|---------|---------|
| `0` | `OK` | All integrity checks pass |
| `1` | `INVALID` | One or more integrity or schema findings |
| `2` | `REFUSAL` | Manifest unreadable, unparseable, or unsupported version |

### inspect

| Exit Code | Outcome | Meaning |
|-----------|---------|---------|
| `0` | `METADATA` | Manifest metadata was read without verifying integrity |
| `2` | `REFUSAL` | Manifest unreadable, unparseable, or unsupported version |

### diff

| Exit Code | Outcome | Meaning |
|-----------|---------|---------|
| `0` | `NO_CHANGES` | Manifests have identical members and hashes |
| `1` | `CHANGES` | Members added, removed, or changed between packs |
| `2` | `REFUSAL` | One or both packs cannot be read |

### archive

| Exit Code | Outcome | Meaning |
|-----------|---------|---------|
| `0` | `ARCHIVE_CREATED` / `ARCHIVE_IMPORTED` | Deterministic tar wrapper exported or imported |
| `2` | `REFUSAL` | Pack or archive is malformed, unsafe, or would overwrite output |

---

## How pack Compares

| Capability | pack | tar + shasum | zip + checksum | Git commit | Custom script |
|------------|------|-------------|----------------|------------|---------------|
| Self-verifiable content ID | Yes (`pack_id`) | No | No | Yes (SHA-1) | You write it |
| Closed-set enforcement | Yes | No | No | No | You write it |
| Artifact type detection | Yes | No | No | No | You write it |
| Schema validation | Yes | No | No | No | You write it |
| Deterministic manifest | Yes | No | No | Yes | You write it |
| Machine-readable verify report | Yes (JSON) | No | No | No | You write it |
| Diff between bundles | Yes | No | No | Yes (`git diff`) | You write it |
| Audit trail (witness ledger) | Yes | No | No | Yes (reflog) | No |

**When to use pack:**
- End of an evidence pipeline — seal artifacts after lockfile generation, shape checks, and reconciliation
- Audit and compliance — produce an immutable, verifiable evidence bundle with content-addressed ID
- CI automation — machine-readable verify reports that gate downstream actions

**When pack might not be ideal:**
- You need streaming or compressed archive verification — archive export/import is a portable wrapper, not a streaming integrity mode
- You need built-in remote transport — pack does not push or pull. Inside CMD+RVL, receipt registration into the metadata catalog is provided by `cmdrvl context pack emit-receipt` (cmdrvl-cli); pack itself is local-only.
- You need signed attestation — pack verifies content integrity, not identity (use `gh attestation` for that)

---

## Installation

### Homebrew (Recommended)

```bash
brew install cmdrvl/tap/pack
```

### Shell Script

```bash
curl -fsSL https://raw.githubusercontent.com/cmdrvl/pack/main/scripts/install.sh | bash
```

### From Source

```bash
cargo build --release
./target/release/pack --help
```

---

## CLI Reference

```bash
pack seal <ARTIFACT>... [OPTIONS]
pack verify <PACK_DIR> [OPTIONS]
pack inspect <PACK_DIR> [OPTIONS]
pack diff <A> <B> [OPTIONS]
pack archive export <PACK_DIR> --out <FILE>
pack archive import <ARCHIVE> --out <DIR>
pack witness <query|last|count> [OPTIONS]
pack --robot-triage
pack capabilities --json
pack robot-docs guide
pack doctor <health|capabilities|robot-docs> [OPTIONS]
pack doctor --robot-triage
```

### seal

Collect artifacts into a sealed pack directory.

```bash
pack seal nov.lock.json dec.lock.json rules.json \
  --output evidence/2025-12/ \
  --note "Q4 reconciliation" \
  --created 2026-01-15T10:30:00Z
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--output <DIR>` | path | `~/.cmdrvl/state/pack/<pack_id>/` | Output directory (must be empty or nonexistent) |
| `--note <TEXT>` | string | none | Human-readable note embedded in manifest |
| `--created <RFC3339>` | timestamp | current UTC or `SOURCE_DATE_EPOCH` | Reproducible manifest `created` timestamp |
| `--outcome <TAG>` | string | none | Optional canonical anchor (`cmdrvl://...`) embedded as `primary_outcome_tag` |
| `--no-witness` | flag | `false` | Suppress witness ledger recording |

Without `--output`, `pack` stages and writes the pack under
`~/.cmdrvl/state/pack/<pack_id>/`. On first use it copies a legacy local
`./pack/` output directory into `~/.cmdrvl/state/pack/` when that canonical
directory does not already exist, and records the migration under
`~/.cmdrvl/migrations/` and `~/.cmdrvl/notices/`.

For reproducible repacks, pass `--created <RFC3339>`. If it is absent, `pack`
honors `SOURCE_DATE_EPOCH` as Unix seconds; if neither is set, `created` uses
the current UTC time. The explicit `--created` flag takes precedence over the
environment.
If provided, `--outcome` must be a canonical IRI beginning with `cmdrvl://`.

### verify

Verify pack integrity — all checks, structured report.

```bash
pack verify evidence/2025-12/              # Human output
pack verify evidence/2025-12/ --json       # Machine-readable JSON
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--json` | flag | `false` | JSON report output |
| `--no-witness` | flag | `false` | Suppress witness ledger recording |

### inspect

Inspect pack metadata without verifying integrity. This is for quick scanning;
use `pack verify` before relying on hashes, closed-set membership, or `pack_id`.
`inspect` does not append witness records.

```bash
pack inspect evidence/2025-12/              # Human metadata output
pack inspect evidence/2025-12/ --json       # Machine-readable metadata
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--json` | flag | `false` | JSON metadata output |

### diff

Deterministically compare two pack manifests.

```bash
pack diff evidence/2025-11/ evidence/2025-12/          # Human output
pack diff evidence/2025-11/ evidence/2025-12/ --json   # JSON report
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--json` | flag | `false` | JSON report output |

### archive

Export or import deterministic uncompressed tar wrappers while keeping the pack
directory as the canonical integrity root. Archives are transport wrappers, not
new trust roots; imported archives are verified before promotion.

```bash
pack archive export evidence/2025-12/ --out evidence/2025-12.pack.tar
pack archive import evidence/2025-12.pack.tar --out recovered/2025-12/
```

Output:

```text
ARCHIVE_CREATED sha256:...
evidence/2025-12.pack.tar

ARCHIVE_IMPORTED sha256:...
recovered/2025-12/
```

### doctor

Read-only diagnostics for agents and operators. Top-level agent discovery
commands and doctor commands do not seal packs, verify integrity, walk pack
directories, import or export archives, append witness records, create witness
directories, write doctor artifacts, rewrite metadata, or use the network.

```bash
pack --robot-triage
pack capabilities --json
pack robot-docs guide
pack doctor health
pack doctor health --json
pack doctor capabilities --json
pack doctor robot-docs
pack doctor --robot-triage
```

No fix mode is available. `pack doctor --fix` exits `2`, emits only stderr, and
names the read-only alternatives.

### Global Flags

| Flag | Description |
|------|-------------|
| `--describe` | Print compiled `operator.json` to stdout, exit `0` |
| `--schema` | Print `pack.v0` JSON schema to stdout, exit `0` |
| `--robot-triage` | Print read-only machine triage JSON to stdout, exit `0` |
| `--version` | Print `pack <semver>` to stdout, exit `0` |
| `--no-witness` | Suppress witness record writes |

### Exit Codes

| Code | seal | verify | inspect | diff | archive |
|------|------|--------|---------|------|---------|
| `0` | `PACK_CREATED` | `OK` | `METADATA` | `NO_CHANGES` | `ARCHIVE_CREATED` / `ARCHIVE_IMPORTED` |
| `1` | — | `INVALID` | — | `CHANGES` | — |
| `2` | `REFUSAL` | `REFUSAL` | `REFUSAL` | `REFUSAL` | `REFUSAL` |

---

## Pack Directory Contract

A sealed pack is a closed-set directory:

```text
evidence/2025-12/
├── manifest.json
├── nov.lock.json
├── dec.lock.json
├── shape.report.json
├── rules.json
└── nested_registry/
    ├── registry.json
    └── loans.csv
```

Rules enforced by `verify`:

- `manifest.json` must exist and parse as `pack.v0`
- `manifest.json` is reserved — cannot be a member path
- Member paths must be safe relative paths (no absolute, no `..`)
- Only declared members plus `manifest.json` are allowed (no extra files)
- `member_count` must match the actual members array length

---

## Deterministic `pack_id`

The self-hash contract:

1. Construct manifest with `pack_id: ""`
2. Serialize to canonical JSON (sorted keys, no whitespace)
3. SHA-256 hash the canonical bytes
4. Set `pack_id` to `sha256:<hex>`

Any change to manifest content — members, note, hashes — changes `pack_id`.

---

## Verify Checks

`pack verify` runs these checks in order:

1. **manifest_parse** — manifest exists and deserializes as `pack.v0`
2. **member_count** — `member_count` field matches members array length
3. **member_paths** — paths are unique, safe, and non-reserved
4. **member_hashes** — each member exists as a regular file with matching SHA-256
5. **extra_members** — no undeclared files beyond `manifest.json`
6. **pack_id** — recomputed `pack_id` matches the declared value
7. **schema_validation** — known artifact types validate against local schemas

JSON report example:

```json
{
  "version": "pack.verify.v0",
  "outcome": "INVALID",
  "pack_id": "sha256:e78de23c...",
  "checks": {
    "manifest_parse": true,
    "member_count": true,
    "member_paths": true,
    "member_hashes": false,
    "extra_members": true,
    "pack_id": true,
    "schema_validation": "pass"
  },
  "invalid": [
    { "code": "MISSING_MEMBER", "path": "rvl.report.json" }
  ]
}
```

---

## Refusal Codes

| Code | Trigger | Next Step |
|------|---------|-----------|
| `E_EMPTY` | No artifacts provided to `seal` | Provide at least one artifact path |
| `E_IO` | Read/write/path I/O failure | Check paths exist and are readable |
| `E_DUPLICATE` | Member path collision | Rename artifacts to have unique basenames |
| `E_BAD_PACK` | Invalid or unreadable manifest | Check manifest.json exists and is valid JSON |

Refusal envelopes are always structured JSON on stdout:

```json
{
  "version": "pack.v0",
  "outcome": "REFUSAL",
  "refusal": {
    "code": "E_IO",
    "message": "Cannot read artifact: /nonexistent/file.json",
    "detail": null,
    "next_command": null
  }
}
```

---

## Troubleshooting

### "E_IO" — cannot read artifact

Check that the file exists and you have read permissions:

```bash
ls -la /path/to/artifact.json
pack seal /path/to/artifact.json --output /tmp/test
```

### "E_DUPLICATE" — member path collision

Two artifacts resolve to the same basename. Rename or restructure:

```bash
# Wrong — both resolve to "data.json":
pack seal dir1/data.json dir2/data.json

# Right — use directories to disambiguate:
pack seal dir1/ dir2/
```

### "E_BAD_PACK" — manifest unreadable

The directory doesn't contain a valid `manifest.json`:

```bash
ls -la evidence/2025-12/manifest.json  # verify it exists
cat evidence/2025-12/manifest.json | jq .  # verify it's valid JSON
```

### verify shows INVALID with HASH_MISMATCH

A member file was modified after sealing. Re-seal with the current files:

```bash
pack verify evidence/2025-12/ --json | jq '.invalid[] | select(.code == "HASH_MISMATCH")'
# Shows which file changed and the expected vs actual hash
```

### verify shows INVALID with EXTRA_MEMBER

An undeclared file was added to the pack directory. Remove it or re-seal:

```bash
pack verify evidence/2025-12/ --json | jq '.invalid[] | select(.code == "EXTRA_MEMBER") | .path'
```

---

## Limitations

| Limitation | Detail |
|------------|--------|
| **Directory-based integrity** | The pack directory remains canonical; archive export/import is only a deterministic transport wrapper |
| **No built-in remote transport** | Pack is local-only. Inside CMD+RVL, receipt registration into the metadata catalog is provided by `cmdrvl context pack emit-receipt` (cmdrvl-cli) |
| **No signing** | pack verifies content integrity, not author identity |
| **No incremental packs** | Each pack is a complete snapshot — no delta packs |
| **No streaming verify** | Entire pack must be on disk — no remote verification |
| **Schema validation is local** | Known artifact types only — custom schemas not yet supported |

---

## FAQ

### Why not just use `tar` + `shasum`?

`tar` doesn't enforce a closed-set contract, doesn't auto-detect artifact types, doesn't produce machine-readable verify reports, and doesn't support deterministic content-addressed IDs. `pack` is purpose-built for evidence integrity.

### Why a directory instead of an archive?

Directories are inspectable without extraction. You can `cat manifest.json`, verify individual members, or browse the evidence set. Archives would add compression/extraction overhead with no integrity benefit.

`pack archive` exists for one-file transport, but it deliberately preserves that model: export verifies the source directory first, writes a deterministic uncompressed tar with `manifest.json` first, and import verifies the extracted directory before promotion.

### How does `pack_id` work?

The manifest is serialized with `pack_id: ""`, keys sorted, no whitespace. SHA-256 of those bytes becomes `sha256:<hex>`. This means any change to any member hash, path, or metadata changes the `pack_id`. It's a Merkle-root-like content address for the entire evidence set.

### Can I include nested directories?

Yes. Pass a directory as an artifact and its contents are recursively included with relative paths preserved (e.g., `nested_registry/loans.csv`).

### What artifact types does pack detect?

Lockfiles (`lock.v0`), reports (`rvl.v0`, `shape.v0`, `verify.v0`, `compare.v0`), rules, profiles (YAML), registries (JSON registries and CSVs in registry paths), and `other` for everything else. Detection uses JSON `version` fields and YAML structure.

Profile typing remains broad: YAML with `schema_version` plus `profile_id` is a
`profile` whether it is draft or frozen. When that profile also declares
`status: frozen` and `profile_sha256`, pack records `profile_frozen: true`,
`profile_sha256`, and any declared `column_registry_hash` on the manifest member
and inspect member output. Draft profiles do not get identity fields fabricated.
Profile `schema_version` is not emitted as `artifact_version`; pack reserves
`artifact_version` for recognized artifact `version` fields used by local JSON
schema validation.

### Does verify modify the pack?

No. `pack verify` is purely read-only. It never modifies the pack directory.

### What happens if the witness ledger is unwritable?

The domain operation (seal/verify) still succeeds with its normal exit code. A warning is printed to stderr. Witness failure is always non-fatal.

---

## Agent / CI Integration

For the full toolchain guide, see the [Agent Operator Guide](https://github.com/cmdrvl/.github/blob/main/profile/AGENT_PROMPT.md).

### Self-describing contract

```bash
$ pack --describe | jq '.exit_codes'
{
  "seal": { "0": "PACK_CREATED", "2": "REFUSAL" },
  "verify": { "0": "OK", "1": "INVALID", "2": "REFUSAL" }
}

$ pack --describe | jq '.pipeline'
{
  "upstream": ["lock", "fingerprint"],
  "downstream": []
}
```

### Agent workflow

```bash
# 1. Seal artifacts
pack seal *.lock.json *.report.json \
  --output evidence/run-42/ --no-witness

case $? in
  0) echo "sealed"
     pack verify evidence/run-42/ --json --no-witness ;;
  2) echo "refusal"
     exit 1 ;;
esac

# 2. Verify and branch on outcome
pack verify evidence/run-42/ --json --no-witness > report.json

case $? in
  0) echo "OK — evidence intact"
     jq '.pack_id' report.json ;;
  1) echo "INVALID"
     jq '.invalid' report.json ;;
  2) echo "REFUSAL"
     exit 1 ;;
esac
```

### What makes this agent-friendly

- **Exit codes** — `0`/`1`/`2` map to success/invalid/error branching
- **Structured JSON only** — `--json` on verify and diff produces machine-readable output
- **`--describe`** — prints `operator.json` so an agent discovers the tool without reading docs
- **`--schema`** — prints the pack JSON Schema for programmatic validation
- **`--robot-triage`** — prints health, capabilities, and recommended actions in one JSON payload
- **`capabilities --json`** — prints command surfaces, side effects, and fix-mode policy
- **`robot-docs guide`** — prints an in-tool operating guide for agents
- **`--no-witness`** — suppresses side effects for isolated CI runs

---

<details>
<summary><strong>Witness Subcommands</strong></summary>

`pack` records seal, verify, and diff outcomes to an ambient witness ledger. Query and count default to `pack` rows, and accept the standard filter surface when you need to narrow or widen the shared-ledger view:

```bash
# Query all records
pack witness query --json

# Query a filtered window
pack witness query --since 2026-01-15T10:00:00Z --outcome PACK_CREATED --json

# Query a different tool in the shared ledger
pack witness query --tool hash --json

# Get the most recent operation
pack witness last --json

# Count operations
pack witness count --outcome REFUSAL --json
```

### Subcommand Reference

```bash
pack witness query [--tool TOOL] [--since RFC3339] [--until RFC3339] [--outcome OUTCOME] [--input-hash HASH] [--json]
pack witness last [--json]
pack witness count [--tool TOOL] [--since RFC3339] [--until RFC3339] [--outcome OUTCOME] [--input-hash HASH] [--json]
```

### Exit Codes (witness subcommands)

| Code | Meaning |
|------|---------|
| `0` | Records returned successfully |
| `2` | CLI parse error or witness internal error |

### Ledger Location

- Default: `~/.cmdrvl/state/witness/witness.jsonl`
- Override: set `EPISTEMIC_WITNESS` environment variable
- First use without an override copies a legacy `~/.epistemic/witness.jsonl`
  ledger into the canonical location when the canonical file does not already
  exist, and records the migration under `~/.cmdrvl/migrations/` and
  `~/.cmdrvl/notices/`.
- Malformed ledger lines are skipped; valid lines continue to be processed.

### Planned Projection

The planned witness-to-pack projection mode treats an NTM session as the natural
unit of work: a swarm or tournament produces a witness trail, then `pack` uses a
bounded slice of that trail to choose artifacts for normal `pack seal`.

Projection is only a selection layer. The pack directory remains the integrity
root, selected files must still exist, recorded hashes must match current bytes,
and the operator must preview the selection before sealing.

</details>

---

## Spec and Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
```

### Performance Baseline

Large-pack profiling lives in [docs/PERF_BASELINE.md](docs/PERF_BASELINE.md).
The harness is ignored by default; run it locally with a release binary:

```bash
cargo build --release
PACK_PERF_BIN=target/release/pack \
cargo test --test perf_baseline -- --ignored --nocapture --test-threads=1
```

Set `PACK_THREADS=1` to force single-threaded seal/verify hashing for debugging
or branch-to-branch performance comparisons.

### Project Structure

```text
src/
├── main.rs          Entry point
├── lib.rs           CLI dispatch
├── cli/             Clap argument parsing, exit codes
├── archive.rs       Deterministic tar export/import wrappers
├── seal/            Seal pipeline: collect, collision, copy, finalize, manifest
├── verify/          Verify pipeline: checks, schema validation, report
├── inspect.rs       Read-only metadata inspection
├── diff/            Diff pipeline: compare manifests, report
├── detect/          Member type detection
├── refusal/         Refusal codes and envelope
├── witness/         Witness ledger append/query
├── operator.rs      --describe output
└── schema.rs        --schema output

tests/
├── cli_scaffold.rs      CLI surface integration tests
├── seal_suite.rs        Seal contract integration tests
├── verify_suite.rs      Verify contract integration tests
├── conformance_matrix.rs Plan-to-test evidence checks
├── refusal_suite.rs     Refusal envelope integration tests
├── schema_validation.rs Schema validation integration tests
└── witness_suite.rs     Witness behavior integration tests

fixtures/
├── artifacts/       Raw input artifacts for seal
├── packs/           Pre-built pack fixtures (valid + 4 invalid variants)
└── schema/          Type detection validation fixtures
```

---

*`pack` is part of the open-source toolchain from the [CMD+RVL](https://cmdrvl.com) lineage and AI enablement practice. MIT-licensed. Contributions welcome from any practice or stack.*
