# pack

Seal lockfiles, reports, rules, and registry artifacts into one immutable, self-verifiable evidence pack.

A pack is the deterministic answer to: *what was known, and how was it established?*

- A directory you can inspect
- A hash you can trust

## Quickstart

```bash
# Install from source
cargo build --release
export PATH="$PWD/target/release:$PATH"

# Seal artifacts into an evidence pack
pack seal nov.lock.json dec.lock.json shape.report.json rvl.report.json \
  --note "Nov→Dec 2025 reconciliation" \
  --output evidence/2025-12/
# → PACK_CREATED sha256:e78de23c...
# → evidence/2025-12/

# Verify the sealed pack
pack verify evidence/2025-12/
# → pack verify: OK
# →   pack_id: sha256:e78de23c...

# Check the witness ledger
pack witness last
# → 2026-02-25T12:00:00.000Z verify OK -
```

### Successful flow (exit 0)

```bash
$ pack seal fixtures/artifacts/nov.lock.json fixtures/artifacts/rules.json \
    --output /tmp/demo --no-witness
PACK_CREATED sha256:abc123...
/tmp/demo

$ pack verify /tmp/demo --no-witness
pack verify: OK
  pack_id: sha256:abc123...
```

### Refusal flow (exit 2)

```bash
$ pack seal /nonexistent/file.json --no-witness
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

$ echo $?
2
```

## Why pack exists

Without `pack`, evidence is fragmented:

- Artifacts live in scattered paths
- No single manifest binds them together
- No content-addressed identifier for the full evidence set
- Verification across the bundle is manual and brittle

With `pack`:

- Members are copied byte-for-byte into one closed-set directory
- `manifest.json` binds all members and metadata
- `pack_id` is deterministic and self-verifiable
- Verification has explicit exit semantics: `OK`, `INVALID`, `REFUSAL`

## CLI surface

```text
pack <COMMAND> [OPTIONS]

Commands:
  seal <ARTIFACT>...           Seal artifacts into a pack directory
  verify <PACK_DIR>            Verify pack integrity
  witness <query|last|count>   Query witness ledger

  diff <A> <B>                 (deferred in v0.1)
  push <PACK_DIR>              (deferred in v0.1)
  pull <PACK_ID> --out DIR     (deferred in v0.1)

Global flags:
  --describe     Print compiled operator.json, exit 0
  --schema       Print pack.v0 JSON schema, exit 0
  --version      Print version, exit 0
  --no-witness   Suppress witness record writes
```

## Exit semantics

| Command | Exit 0 | Exit 1 | Exit 2 |
|---------|--------|--------|--------|
| `seal` | `PACK_CREATED` | — | `REFUSAL` |
| `verify` | `OK` | `INVALID` | `REFUSAL` |

## v0.1 boundaries

### Ships in v0.1

- `pack seal` — collect, hash, type-detect, seal into closed-set directory
- `pack verify` — integrity checks, schema validation, deterministic findings
- `pack witness` — append-only ledger query (`query`, `last`, `count`)
- Deterministic `pack_id` self-hash contract
- Refusal system: `E_EMPTY`, `E_IO`, `E_DUPLICATE`, `E_BAD_PACK`
- Global flags: `--describe`, `--schema`, `--version`, `--no-witness`

### Deferred past v0.1

- `pack diff` — compare two packs
- `pack push` — upload to data-fabric
- `pack pull` — download from data-fabric

These commands exist in the CLI surface but exit 2 with a deferred message.

## Witness defaults

By default, `seal` and `verify` append a record to the witness ledger:

| Location | Priority |
|----------|----------|
| `$EPISTEMIC_WITNESS` | Checked first |
| `~/.epistemic/witness.jsonl` | Fallback default |

Key behaviors:

- **Witness failure is non-fatal**: if the ledger is unwritable, a warning prints to stderr but the domain exit code is preserved.
- **`--no-witness`** suppresses all witness writes for the invocation.
- **Witness query commands** (`witness query`, `witness last`, `witness count`) do not themselves write witness records.

## Pack directory contract

A sealed pack is a closed-set directory:

```text
evidence/2025-12/
├── manifest.json
├── nov.lock.json
├── dec.lock.json
├── shape.report.json
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

## Deterministic `pack_id`

The self-hash contract:

1. Construct manifest with `pack_id: ""`
2. Serialize to canonical JSON (sorted keys, no whitespace)
3. SHA256 hash the canonical bytes
4. Set `pack_id` to `sha256:<hex>`

Any change to manifest content changes `pack_id`.

## Verify checks

`pack verify` runs these checks in order:

1. **manifest_parse** — manifest exists and deserializes as `pack.v0`
2. **member_count** — `member_count` field matches members array length
3. **member_paths** — paths are unique, safe, and non-reserved
4. **member_hashes** — each member exists as a regular file with matching SHA256
5. **extra_members** — no undeclared files beyond `manifest.json`
6. **pack_id** — recomputed `pack_id` matches the declared value
7. **schema_validation** — known artifact types validate against local schemas

Outcomes:

- `OK` (exit 0): all checks pass
- `INVALID` (exit 1): one or more integrity or schema findings
- `REFUSAL` (exit 2): manifest unreadable, unparseable, or unsupported version

## Refusal codes

| Code | Meaning |
|------|---------|
| `E_EMPTY` | No artifacts provided to `seal` |
| `E_IO` | Read/write/path IO failure |
| `E_DUPLICATE` | Member path collision (including reserved path) |
| `E_BAD_PACK` | Invalid or unreadable manifest |

## Development

```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo test -- --test-threads=1       # All tests (single-threaded for witness tests)
cargo clippy --all-targets           # Lint
cargo fmt --check                    # Format check
```

## Project structure

```text
src/
├── main.rs          Entry point
├── lib.rs           CLI dispatch
├── cli/             Clap argument parsing, exit codes
├── seal/            Seal pipeline: collect, collision, copy, finalize, manifest
├── verify/          Verify pipeline: checks, schema validation, report
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
