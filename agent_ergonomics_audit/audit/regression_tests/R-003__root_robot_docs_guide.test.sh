#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TOOL="${TOOL:-$ROOT/target/debug/pack}"

stdout="$("$TOOL" robot-docs guide)"

grep -Fq "pack --robot-triage" <<<"$stdout"
grep -Fq "pack capabilities --json" <<<"$stdout"
grep -Fq "pack robot-docs guide" <<<"$stdout"
grep -Fq 'pack doctor --fix` is unavailable' <<<"$stdout"
