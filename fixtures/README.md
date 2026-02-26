# fixtures/

Deterministic fixture corpus for `pack` seal, verify, refusal, and type-detection test suites.

All fixture bytes are committed as-is and must remain stable across test runs.

---

## artifacts/

Raw input files for `pack seal`. These are the source materials that get collected, hashed, and sealed into a pack.

| File | Detected Type | Version | Test Families |
|------|--------------|---------|---------------|
| `nov.lock.json` | lockfile | lock.v0 | seal, type-detection |
| `dec.lock.json` | lockfile | lock.v0 | seal, type-detection |
| `shape.report.json` | report | shape.v0 | seal, type-detection |
| `rvl.report.json` | report | rvl.v0 | seal, type-detection |
| `verify.report.json` | report | verify.v0 | seal, type-detection |
| `rules.json` | rules | verify.rules.v0 | seal, type-detection |
| `profile.yaml` | profile | — | seal, type-detection |
| `unknown.txt` | other | — | seal, type-detection |
| `nested_registry/` | registry | — | seal (directory input), type-detection |

---

## packs/

Pre-built pack directories for verify and refusal integration tests.

### packs/valid/

A complete, correctly-sealed evidence pack created from `artifacts/`. Verifies as `OK` (exit 0).

- **Test families**: verify-ok, seal round-trip, witness integration
- **pack_id**: `sha256:e78de23c97bc6b7637ee9196c77ad91f7fe0383c4753f95861cbfc9719e20875`
- **member_count**: 10

### packs/missing_member/

Copy of `valid/` with `rvl.report.json` deleted. Manifest still declares it.

- **Expected finding**: `MISSING_MEMBER` on `rvl.report.json`
- **Exit code**: 1 (INVALID)
- **Test families**: verify-invalid, refusal integration

### packs/tampered_member/

Copy of `valid/` with `rvl.report.json` content overwritten. Hash no longer matches.

- **Expected finding**: `HASH_MISMATCH` on `rvl.report.json`
- **Exit code**: 1 (INVALID)
- **Test families**: verify-invalid, refusal integration

### packs/tampered_manifest/

Copy of `valid/` with the `note` field changed in `manifest.json`. The `pack_id` is now stale.

- **Expected finding**: `PACK_ID_MISMATCH`
- **Exit code**: 1 (INVALID)
- **Test families**: verify-invalid, refusal integration

### packs/extra_member/

Copy of `valid/` with an extra file `undeclared.txt` not declared in the manifest.

- **Expected finding**: `EXTRA_MEMBER` on `undeclared.txt`
- **Exit code**: 1 (INVALID)
- **Test families**: verify-invalid, refusal integration

---

## schema/

Fixtures for member type detection validation, organized by expected outcome.

### schema/expected.json

Annotation file mapping each fixture path (relative to `schema/`) to its expected `member_type` and `artifact_version`. Tests can iterate this file to validate all fixtures in a single loop.

### schema/pass/

Artifacts with recognized version markers. Detection should return the correct type.

| File | Expected Type | Expected Version |
|------|--------------|-----------------|
| `lockfile.json` | lockfile | lock.v0 |
| `report_rvl.json` | report | rvl.v0 |
| `report_shape.json` | report | shape.v0 |
| `report_verify.json` | report | verify.v0 |
| `report_compare.json` | report | compare.v0 |
| `artifact_canon.json` | artifact | canon.v0 |
| `artifact_assess.json` | artifact | assess.v0 |
| `rules.json` | rules | verify.rules.v0 |
| `profile.yaml` | profile | — |

### schema/fail/

Artifacts where detection should NOT match a known type (falls through to `other`).

| File | Reason | Expected Type |
|------|--------|--------------|
| `unknown_version.json` | Unrecognized version string | other |
| `no_version_field.json` | No `version` field | other |
| `yaml_missing_profile_id.yaml` | Has `schema_version` but no `profile_id` | other |
| `yaml_missing_schema_version.yaml` | Has `profile_id` but no `schema_version` | other |

### schema/skipped/

Content where all parsers are skipped (not JSON, not YAML profile, not registry path).

| File | Reason | Expected Type |
|------|--------|--------------|
| `plain.txt` | Plain text, no markers | other |
| `binary.bin` | Binary content, not valid UTF-8 | other |

---

## Non-Regular Member Strategy

Symlinks and non-regular files cannot be reliably committed to git or created in CI across platforms. Instead of fixture files, test suites handle non-regular members via:

1. **Runtime creation in temp dirs**: Tests that validate symlink/FIFO detection create them in `TempDir` at test time (see `seal::collect` and `verify::checks` test modules).
2. **Guard pattern**: `#[cfg(unix)]` gates for FIFO and symlink tests, ensuring CI-safe cross-platform behavior.
3. **No committed symlinks**: This avoids git `core.symlinks` issues on Windows and keeps the fixture corpus fully portable.

---

## Regenerating Fixtures

The `packs/valid/` fixture can be regenerated from `artifacts/`:

```bash
rm -rf fixtures/packs/valid
pack seal fixtures/artifacts/nov.lock.json \
  fixtures/artifacts/dec.lock.json \
  fixtures/artifacts/shape.report.json \
  fixtures/artifacts/rvl.report.json \
  fixtures/artifacts/verify.report.json \
  fixtures/artifacts/rules.json \
  fixtures/artifacts/profile.yaml \
  fixtures/artifacts/unknown.txt \
  fixtures/artifacts/nested_registry \
  --output fixtures/packs/valid \
  --note "fixture: valid evidence pack" \
  --no-witness
```

**Note**: Regeneration changes the `created` timestamp and therefore the `pack_id`. After regeneration, update this README and any hardcoded pack_id values in tests, then rebuild the invalid pack variants by copying and modifying `valid/`.
