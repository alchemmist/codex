# Antex workflows extension

Install it with `antex extensions install workflows`. Project workflows live in
`.antex/workflows/<id>.py` and expose a `WORKFLOW` manifest plus `run(ctx)`.

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
