# pack

<div align="center">

[![CI](https://github.com/cmdrvl/pack/actions/workflows/ci.yml/badge.svg)](https://github.com/cmdrvl/pack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub release](https://img.shields.io/github/v/release/cmdrvl/pack)](https://github.com/cmdrvl/pack/releases)

**Immutable evidence packs — seal lockfiles, reports, rules, and registry artifacts into one self-verifiable, content-addressed directory.**

No AI. No inference. Pure deterministic sealing, hashing, and verification.

```bash
brew install cmdrvl/tap/pack
```

</div>

---

## TL;DR

**The Problem**: After a pipeline produces lockfiles, reports, and rules, evidence is fragmented — artifacts live in scattered paths, nothing binds them together, and there's no content-addressed identifier for the full evidence set. Verification across the bundle is manual and brittle.

**The Solution**: One command that collects artifacts into a closed-set directory with a `manifest.json`, computes a deterministic `pack_id` via self-hash, and produces an immutable evidence pack. Another command verifies it bit-for-bit.

### Why Use pack?

| Feature | What It Does |
|---------|--------------|
| **Self-verifiable** | `pack_id` is the SHA-256 of canonical manifest with `pack_id=""` — any change invalidates it |
| **Closed-set** | Only declared members plus `manifest.json` allowed — no undeclared files |
| **Deterministic** | Same artifacts always produce the same `pack_id` and manifest |
| **Type detection** | Auto-classifies lockfiles, reports, rules, profiles, registries from content |
| **Schema validation** | Verifies known artifact types against local schemas |
| **Structured outcomes** | `OK` / `INVALID` / `REFUSAL` with machine-readable JSON reports |
| **Diff** | Deterministic comparison of two pack manifests — added, removed, changed members |
| **Audit trail** | Every seal and verify recorded in the ambient witness ledger |

---

## Quick Example

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

# Full pipeline from scan to sealed pack
$ vacuum /data/dec | hash | lock --dataset-id "dec" > dec.lock.json
$ pack seal dec.lock.json shape.report.json --output evidence/dec/
```

---

## Where pack Fits

`pack` is the **final tool** in the stream pipeline — it seals everything into an immutable evidence set.

```
vacuum  →  hash  →  fingerprint  →  lock  →  pack
(scan)    (hash)    (template)     (pin)    (seal)
```

Upstream tools produce individual artifacts (lockfiles, reports). `pack` collects them into a single verifiable bundle with a content-addressed identifier.

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

### diff

| Exit Code | Outcome | Meaning |
|-----------|---------|---------|
| `0` | `NO_CHANGES` | Manifests have identical members and hashes |
| `1` | `CHANGES` | Members added, removed, or changed between packs |
| `2` | `REFUSAL` | One or both packs cannot be read |

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
- You need streaming archives — pack is a directory, not a tarball
- You need network distribution — `push`/`pull` are deferred in v0.1
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
pack diff <A> <B> [OPTIONS]
pack witness <query|last|count> [OPTIONS]
```

### seal

Collect artifacts into a sealed pack directory.

```bash
pack seal nov.lock.json dec.lock.json rules.json \
  --output evidence/2025-12/ \
  --note "Q4 reconciliation"
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--output <DIR>` | path | auto-generated | Output directory (must be empty or nonexistent) |
| `--note <TEXT>` | string | none | Human-readable note embedded in manifest |
| `--no-witness` | flag | `false` | Suppress witness ledger recording |

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

### diff

Deterministically compare two pack manifests.

```bash
pack diff evidence/2025-11/ evidence/2025-12/          # Human output
pack diff evidence/2025-11/ evidence/2025-12/ --json   # JSON report
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--json` | flag | `false` | JSON report output |

### Global Flags

| Flag | Description |
|------|-------------|
| `--describe` | Print compiled `operator.json` to stdout, exit `0` |
| `--schema` | Print `pack.v0` JSON schema to stdout, exit `0` |
| `--version` | Print `pack <semver>` to stdout, exit `0` |
| `--no-witness` | Suppress witness record writes |

### Exit Codes

| Code | seal | verify | diff |
|------|------|--------|------|
| `0` | `PACK_CREATED` | `OK` | `NO_CHANGES` |
| `1` | — | `INVALID` | `CHANGES` |
| `2` | `REFUSAL` | `REFUSAL` | `REFUSAL` |

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
| **Directory-based** | Packs are directories, not archives — no tar/zip output |
| **No network transport** | `push`/`pull` deferred in v0.1 — copy directories manually |
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

### How does `pack_id` work?

The manifest is serialized with `pack_id: ""`, keys sorted, no whitespace. SHA-256 of those bytes becomes `sha256:<hex>`. This means any change to any member hash, path, or metadata changes the `pack_id`. It's a Merkle-root-like content address for the entire evidence set.

### Can I include nested directories?

Yes. Pass a directory as an artifact and its contents are recursively included with relative paths preserved (e.g., `nested_registry/loans.csv`).

### What artifact types does pack detect?

Lockfiles (`lock.v0`), reports (`rvl.v0`, `shape.v0`, `verify.v0`, `compare.v0`), rules, profiles (YAML), registries (JSON registries and CSVs in registry paths), and `other` for everything else. Detection uses JSON `version` fields and YAML structure.

### Does verify modify the pack?

No. `pack verify` is purely read-only. It never modifies the pack directory.

### What happens if the witness ledger is unwritable?

The domain operation (seal/verify) still succeeds with its normal exit code. A warning is printed to stderr. Witness failure is always non-fatal.

---

## Agent / CI Integration

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
- **`--no-witness`** — suppresses side effects for isolated CI runs

---

<details>
<summary><strong>Witness Subcommands</strong></summary>

`pack` records every seal and verify to an ambient witness ledger. You can query this ledger:

```bash
# Query all records
pack witness query --json

# Get the most recent operation
pack witness last --json

# Count operations
pack witness count --json
```

### Subcommand Reference

```bash
pack witness query [--json]
pack witness last [--json]
pack witness count [--json]
```

### Exit Codes (witness subcommands)

| Code | Meaning |
|------|---------|
| `0` | Records returned successfully |
| `2` | CLI parse error or witness internal error |

### Ledger Location

- Default: `~/.epistemic/witness.jsonl`
- Override: set `EPISTEMIC_WITNESS` environment variable
- Malformed ledger lines are skipped; valid lines continue to be processed.

</details>

---

## Spec and Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
```

### Project Structure

```text
src/
├── main.rs          Entry point
├── lib.rs           CLI dispatch
├── cli/             Clap argument parsing, exit codes
├── seal/            Seal pipeline: collect, collision, copy, finalize, manifest
├── verify/          Verify pipeline: checks, schema validation, report
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
├── refusal_suite.rs     Refusal envelope integration tests
├── schema_validation.rs Schema validation integration tests
└── witness_suite.rs     Witness behavior integration tests

fixtures/
├── artifacts/       Raw input artifacts for seal
├── packs/           Pre-built pack fixtures (valid + 4 invalid variants)
└── schema/          Type detection validation fixtures
```
