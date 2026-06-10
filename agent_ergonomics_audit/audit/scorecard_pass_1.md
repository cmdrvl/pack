# pack Agent Ergonomics Scorecard - Pass 1

## Applied

- `pack --robot-triage`: one-call JSON triage.
- `pack capabilities --json`: machine-readable command and side-effect contract.
- `pack robot-docs guide`: in-tool agent operating guide.
- `pack doctor --fix`: safe refusal instead of a raw Clap error.

## Deferred

- Generalized typo recovery for common misspelled flags and subcommands.

## Preflight

`bash scripts/preflight.sh /Users/zac/Source/cmdrvl/pack` failed only on missing
`flock`, which is not present on this macOS host. The pass proceeded
single-agent with a narrow write set.
