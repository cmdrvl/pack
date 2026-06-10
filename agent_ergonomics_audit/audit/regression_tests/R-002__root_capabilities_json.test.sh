#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TOOL="${TOOL:-$ROOT/target/debug/pack}"

stdout="$("$TOOL" capabilities --json)"

jq -e '
  .schema_version == "pack.doctor.capabilities.v1" and
  .read_only == true and
  .agent_surfaces.capabilities.command == "pack capabilities --json" and
  .agent_surfaces.robot_docs.command == "pack robot-docs guide" and
  .side_effects.by_command["pack capabilities --json"].writes_witness_ledger == false
' >/dev/null <<<"$stdout"
