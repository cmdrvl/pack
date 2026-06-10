# Agent Ergonomics Audit Workspace

For tool: `pack`
Target: `/Users/zac/Source/cmdrvl/pack`

This is a measurement workspace produced by the
`agent-ergonomics-and-intuitiveness-maximization-for-cli-tools` skill.

## Layout

- `audit/manifest.json` - entry point for this pass
- `audit/surface_inventory.jsonl` - discovered agent surfaces
- `audit/agent_surfaces.jsonl` - scored surfaces
- `audit/recommendations.jsonl` - ranked applied/deferred recommendations
- `audit/applied_changes.jsonl` - applied changes and validation hooks
- `audit/regression_tests/` - shell regression tests for applied changes
- `audit/HANDOFF.md` - resume notes for the next pass

## How to resume

The skill preflight reported missing `flock` on macOS, so this pass proceeded
single-agent. From the skill repo root, run:

1. `bash scripts/preflight.sh /Users/zac/Source/cmdrvl/pack`
2. `bash scripts/discover-cli.sh /Users/zac/Source/cmdrvl/pack`
3. `bash scripts/validate_pass.sh /Users/zac/Source/cmdrvl/pack/agent_ergonomics_audit`
4. Read `audit/HANDOFF.md`.
