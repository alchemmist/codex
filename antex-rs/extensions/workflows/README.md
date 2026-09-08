# Antex workflows extension

Install it with `antex extensions install workflows`. Project workflows live in
`.antex/workflows/<id>.py` and expose a `WORKFLOW` manifest plus `run(ctx)`.
Project discovery requires `trust_project_extensions = true`. Personal workflows
live in `~/.antex/workflows/<id>.py` (or the selected Antex home's `workflows/`).
Duplicate IDs fail explicitly, and symlinked files are excluded. `/workflow list`
lists the available IDs without importing their Python code.
`/workflow` opens the TUI picker. `/workflow status` reports the latest run.
Execution runs in the background, leaving the composer and ordinary model turns
available. `/workflow pause` prevents the next host action from starting; the
current action may finish. `/workflow resume` continues a paused run.
`/workflow stop` terminates the active action and the workflow process, retaining
the latest checkpoint. Stop a workflow before changing model, workspace or session.

```python
WORKFLOW = {"id": "check", "title": "Check"}

def run(ctx):
    result = ctx.shell(["make", "check"])
    if result["exitCode"] != 0:
        return ctx.agent("Fix the failing checks")
    return {"ok": True}
```

Run it with `/workflow check` or pass a JSON parameter object after the ID.
Python owns ordering and branching. Antex executes shell, inspection, and agent
actions with bounded output and the extension's declared capabilities.

Each new run starts with empty checkpoint state and records its parameters,
unique run ID, source snapshot, SHA-256, timestamps and completion/failure state.
Source snapshots are limited to 4096 bytes; the complete persisted record is
limited to 8000 bytes. Agent batches honor `parallelism` between one and eight.
Unsupported agent options produce an error instead of being silently ignored.

`/workflow resume` explicitly restarts an interrupted or failed run using its
saved source and checkpoint. The workflow function starts at the beginning;
use `ctx.state` to skip already completed steps. It must not assume automatic
replay or exactly-once external effects. The workflow must still be discoverable
under the current trust policy. A normal `/workflow <id>` starts a new run.
Inspection and agent project context use the snapshot captured at workflow start.
`print()` and `ctx.log()` go to extension stderr; stdout remains protocol-only.
