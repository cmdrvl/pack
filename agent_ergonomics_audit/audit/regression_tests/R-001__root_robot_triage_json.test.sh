#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TOOL="${TOOL:-$ROOT/target/debug/pack}"

stdout="$("$TOOL" --robot-triage)"

jq -e '
  .schema_version == "pack.doctor.triage.v1" and
  .ok == true and
  .capabilities.agent_surfaces.robot_triage.command == "pack --robot-triage" and
  .capabilities.agent_surfaces.capabilities.command == "pack capabilities --json"
' >/dev/null <<<"$stdout"
