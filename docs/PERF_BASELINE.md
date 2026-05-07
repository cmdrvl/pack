# pack performance baseline

`bd-12s` adds a reproducible local performance harness before optimization work
such as parallel hashing. The harness is intentionally ignored in normal CI
because it creates many temporary files and its numbers depend on local disk,
CPU, thermal state, and whether the debug or release binary is used.

## Run the Baseline

Build a release binary first:

```bash
cargo build --release
```

Run the moderate default profile:

```bash
PACK_PERF_BIN=target/release/pack \
cargo test --test perf_baseline -- --ignored --nocapture --test-threads=1
```

Scale the profile when you want a large-pack run:

```bash
PACK_PERF_BIN=target/release/pack \
PACK_PERF_SMALL_FILES=100000 \
PACK_PERF_SMALL_BYTES=1024 \
PACK_PERF_LARGE_FILES=64 \
PACK_PERF_LARGE_BYTES=10485760 \
cargo test --test perf_baseline -- --ignored --nocapture --test-threads=1
```

Set `PACK_PERF_RSS=0` to disable the `/usr/bin/time` wrapper if it is not
available or if you only want elapsed timings.

`pack` uses available CPU cores for seal/verify member hashing by default. Set
`PACK_THREADS=1` to force single-threaded behavior, or set `PACK_THREADS=<N>` to
cap worker concurrency:

```bash
PACK_THREADS=1 PACK_PERF_BIN=target/release/pack \
cargo test --test perf_baseline -- --ignored --nocapture --test-threads=1
```

## What It Measures

The report schema is `pack.perf-baseline.v0`. It covers two scenarios:

- `many_small_files`: default 1,000 files at 1 KiB each.
- `few_large_files`: default 8 files at 2 MiB each.

Each scenario measures:

- `seal_original`
- `seal_repeat`
- `verify_original`
- `seal_changed`
- `diff_identical`
- `diff_changed`

Each operation reports exit code, elapsed milliseconds, throughput in MiB/s
where byte volume is meaningful, and `max_rss_kib` when `/usr/bin/time` is
available. The report also records `PACK_THREADS` when set. macOS uses
`/usr/bin/time -l`; Linux uses `/usr/bin/time -v`.

The harness also asserts determinism:

- sealing identical inputs with the same `--created` value produces identical
  manifest bytes;
- repeated seals produce the same `pack_id`;
- changing one member byte changes the `pack_id`;
- identical-pack diff returns `NO_CHANGES`;
- changed-pack diff returns `CHANGES`.

## Baseline Note

The default profile is a calibration run, not a release threshold. Its purpose
is to freeze the report format and create a repeatable local command before
optimization beads set targets. Store run output with the machine profile when
comparing branches, for example:

```bash
PACK_PERF_BIN=target/release/pack \
cargo test --test perf_baseline -- --ignored --nocapture --test-threads=1 \
  | tee /tmp/pack-perf-baseline.json
```

Pre-parallel calibration from `bd-12s` on an Apple Silicon macOS workstation
using `target/release/pack` and the default profile:

| Scenario | Operation | Files | Bytes | Duration ms | Throughput MiB/s | Max RSS KiB |
|---|---:|---:|---:|---:|---:|---:|
| many_small_files | seal_original | 1,000 | 1,024,000 | 820 | 1.190 | 8,688 |
| many_small_files | seal_repeat | 1,000 | 1,024,000 | 296 | 3.291 | 7,944 |
| many_small_files | verify_original | 1,000 | 1,024,000 | 90 | 10.838 | 5,380 |
| many_small_files | seal_changed | 1,000 | 1,024,000 | 286 | 3.404 | 7,772 |
| many_small_files | diff_identical | 1,000 | 1,024,000 | 13 | n/a | 3,316 |
| many_small_files | diff_changed | 1,000 | 1,024,000 | 13 | n/a | 3,260 |
| few_large_files | seal_original | 8 | 16,777,216 | 149 | 107.043 | 4,956 |
| few_large_files | seal_repeat | 8 | 16,777,216 | 147 | 108.254 | 5,080 |
| few_large_files | verify_original | 8 | 16,777,216 | 100 | 159.927 | 5,176 |
| few_large_files | seal_changed | 8 | 16,777,216 | 159 | 100.179 | 5,020 |
| few_large_files | diff_identical | 8 | 16,777,216 | 13 | n/a | 2,688 |
| few_large_files | diff_changed | 8 | 16,777,216 | 12 | n/a | 2,920 |

Post-parallel calibration from `bd-2j3` on the same machine, default workers
versus forced single-thread mode:

| Scenario | Operation | Default ms | Default MiB/s | `PACK_THREADS=1` ms | `PACK_THREADS=1` MiB/s |
|---|---:|---:|---:|---:|---:|
| many_small_files | seal_original | 600 | 1.626 | 281 | 3.473 |
| many_small_files | seal_repeat | 156 | 6.225 | 301 | 3.240 |
| many_small_files | verify_original | 45 | 21.512 | 100 | 9.754 |
| many_small_files | seal_changed | 131 | 7.401 | 288 | 3.386 |
| few_large_files | seal_original | 44 | 362.534 | 152 | 104.678 |
| few_large_files | seal_repeat | 49 | 326.154 | 142 | 112.389 |
| few_large_files | verify_original | 31 | 506.701 | 102 | 156.312 |
| few_large_files | seal_changed | 49 | 321.276 | 155 | 102.758 |

The first many-small `seal_original` row is cold-cache and slower in this local
run; the repeated seal, changed seal, verify path, and few-large scenario show
the intended improvement. Use the full JSON report rather than a single row
when comparing branches.
