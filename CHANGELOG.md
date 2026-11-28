# Changelog

## 0.8.0 - 2026-11-27

Versions 0.6.0 to 0.7.2 have no entries here; see their GitHub releases.

- New: `pack verify --expect-id <PACK_ID>` passes only when the pack is intact and its content has that id.
- `pack verify` now accepts `verify ground` reports whose `constraint_hash` is keyed or absent, including ground refusal reports. `verify run` reports are checked as before.
- Library: `VerifyChecks` has a new public field, so code that builds the struct directly must set it.
- Internal: manifest and witness serialization and in-memory hashing come from `cmdrvl-core`; sealed bytes match 0.7.2 in the compatibility tests. The crate forbids `unsafe`.
- Known issue (also in earlier releases): numbers in a source report are read as 64-bit floating point, so a JSON decimal with more than about 16 significant digits can be shown rounded in rendered output. Sealed file bytes and their hashes are not affected.

## 0.5.0 - 2026-05-10

- Removed `pack push` and `pack pull` command surface from the `pack` binary.
- Deleted `src/network/` and dropped the `PACK_DATA_FABRIC_*` environment contract.
- `pack` remains local-first and self-sufficient for `seal`, `verify`, `inspect`, `diff`, `archive`, and `witness`.
- For receipt registration and retrieval, use `cmdrvl context pack emit-receipt` and `cmdrvl context pack pull-receipt`.
