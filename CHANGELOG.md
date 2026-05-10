# Changelog

## 0.5.0 - 2026-05-10

- Removed `pack push` and `pack pull` command surface from the `pack` binary.
- Deleted `src/network/` and dropped the `PACK_DATA_FABRIC_*` environment contract.
- `pack` remains local-first and self-sufficient for `seal`, `verify`, `inspect`, `diff`, `archive`, and `witness`.
- For receipt registration and retrieval, use `cmdrvl context pack emit-receipt` and `cmdrvl context pack pull-receipt`.
