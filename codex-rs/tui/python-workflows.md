# Python workflows (fork feature)

Python workflows are restartable orchestration programs for repetitive tasks that are too large
for one model context. Open the picker with `/workflow`. Project workflows live in
`.codex/workflows/*.py`; personal workflows live in `~/.codex/workflows/*.py`. A project workflow
with the same `id` overrides a personal or built-in workflow.

Each file exposes a `WORKFLOW` manifest and `run(ctx)`:

```python
WORKFLOW = {
    "id": "my-cleanup",
    "title": "My cleanup",
    "description": "Fix one independent issue per agent.",
    "fields": [
        {
            "id": "scope",
            "label": "Scope",
            "type": "text",
            "default": "src",
            "required": True,
        },
        {
            "id": "parallelism",
            "label": "Parallel agents",
            "type": "integer",
            "min": 1,
            "max": 8,
            "default": 4,
        },
        {
            "id": "verify",
            "label": "Verify when finished",
            "type": "boolean",
            "default": True,
        },
    ],
    "guardrails": {
        "max_agent_calls": 1000,
        "max_shell_calls": 1000,
        "max_parallel_agents": 8,
        "timeout_seconds": 43200,
    },
}


def run(ctx):
    report = ctx.shell(["my-linter", "--json", ctx.params["scope"]])
    issues = parse_report(report["stdout"])
    prompts = [prompt_for(issue) for issue in issues]
    results = ctx.agent_batch(prompts, parallelism=ctx.params["parallelism"])
    ctx.checkpoint({"completed": len(results)})
    ctx.progress("Finished one wave", current=len(results), total=len(prompts))
    return {"fixed": sum(result["success"] for result in results)}
```

The context API is synchronous on purpose: Python describes order and branching while Codex owns
the async execution machinery.

- `ctx.params`: answers collected by the TUI from manifest fields.
- `ctx.state`: the last JSON checkpoint, or an empty object for a new run.
- `ctx.progress(message, current=None, total=None)`: update the TUI status row.
- `ctx.shell(argv, cwd=None, timeout_seconds=None, env=None)`: run a bounded command. `argv` must be
  a list; string shell evaluation is deliberately not implicit.
- `ctx.agent(prompt, model=None, reasoning_effort=None, developer_instructions=None, forbid_quality_graph_ignore=False, cwd=None, timeout_seconds=None)`: run one ephemeral Codex agent.
- `ctx.agent_batch(prompts, parallelism=None, model=None, reasoning_effort=None, developer_instructions=None, forbid_quality_graph_ignore=False, cwd=None, timeout_seconds=None)`: run
  independent ephemeral agents concurrently. A prompt can be a string or a dictionary containing
  `prompt`, `model`, `reasoning_effort`, `developer_instructions`, `forbid_quality_graph_ignore`, `cwd`, and `timeout_seconds`.
- `ctx.checkpoint(json_value)`: persist at most 1 MiB of restart state.
- `ctx.log(message)`: write a diagnostic to the Codex log without corrupting the protocol stream.

Text, integer, boolean, and select fields are supported. The manifest controls labels, descriptions,
defaults, ranges, and choices, so each workflow owns its setup UX without owning terminal rendering.

`/workflow pause` stops the current process at the nearest host action and retains its checkpoint.
`/workflow stop` cancels it but also keeps the checkpoint for diagnosis or an explicit resume.
`/workflow resume` lists paused, failed, cancelled, and runs interrupted by a previous Codex exit.

Agent calls invoke the same locally installed `codex exec` binary with the same `CODEX_HOME`, login,
and subscription. Each invocation is ephemeral and starts with a small context. Rust enforces the
manifest guardrails, bounds protocol/output sizes, prevents workflow actions from changing their cwd
outside the current workspace, and snapshots the Python source into every run directory.

Workflow files are trusted local code. Python itself is not sandboxed and can call arbitrary Python
or operating-system APIs, so Rust guardrails constrain the `ctx` API but are not a security boundary.
Only install or run workflows whose source you trust.
