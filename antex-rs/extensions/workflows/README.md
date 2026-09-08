# Antex workflows extension

Install it with `antex extensions install workflows`. Project workflows live in
`.antex/workflows/<id>.py` and expose a `WORKFLOW` manifest plus `run(ctx)`.
Project discovery requires `trust_project_extensions = true`. Personal workflows
live in `~/.antex/workflows/<id>.py` (or the selected Antex home's `workflows/`).
Duplicate IDs fail explicitly, and symlinked files are excluded. `/workflow list`
lists the available IDs without importing their Python code.

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
