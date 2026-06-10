#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TOOL="${TOOL:-$ROOT/target/debug/pack}"

set +e
stderr="$("$TOOL" doctor --fix 2>&1 >/dev/null)"
status=$?
stdout="$("$TOOL" doctor --fix 2>/dev/null || true)"
set -e

test "$status" -eq 2
test -z "$stdout"
grep -Fq "pack doctor --fix is unavailable" <<<"$stderr"
grep -Fq "pack --robot-triage" <<<"$stderr"
grep -Fq "pack capabilities --json" <<<"$stderr"
grep -Fq "pack robot-docs guide" <<<"$stderr"
