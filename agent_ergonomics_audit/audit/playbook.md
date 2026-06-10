# pack Agent Ergonomics Playbook

Use these surfaces before reading source:

```bash
pack --robot-triage
pack capabilities --json
pack robot-docs guide
```

For integrity work, use the domain commands:

```bash
pack seal <ARTIFACT>... --output <DIR> --no-witness
pack verify <DIR> --json --no-witness
pack inspect <DIR> --json
pack diff <A> <B> --json --no-witness
```

Repair mode is intentionally unavailable:

```bash
pack doctor --fix
```

That command must exit `2`, emit no stdout, and name read-only alternatives on
stderr.
