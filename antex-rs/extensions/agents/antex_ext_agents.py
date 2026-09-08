#!/usr/bin/env python3
import json
import sys


MAX_FRAME_BYTES = 256 * 1024
MAX_TEXT_BYTES = 8000


def respond(request, result=None, error=None):
    message = {"jsonrpc": "2.0", "id": request.get("id", "")}
    if error is None:
        message["result"] = result
    else:
        message["error"] = {"code": -32000, "message": str(error)[:512]}
    print(json.dumps(message, separators=(",", ":")), flush=True)


def output(text="", records=None, actions=None, status=None, panel=None):
    value = {"text": text, "records": records or [], "actions": actions or []}
    if status is not None:
        value["status"] = status
    if panel is not None:
        value["panel"] = panel
    return value


def main():
    state = {"runs": []}
    pending = []
    results = []
    for line in sys.stdin:
        if len(line.encode()) > MAX_FRAME_BYTES:
            raise RuntimeError("extension request exceeds its frame budget")
        request = json.loads(line)
        try:
            method = request["method"]
            params = request["params"]
            if method == "initialize":
                if params["protocolVersion"] != 1:
                    raise RuntimeError("incompatible extension protocol version")
                restored = params.get("state")
                if isinstance(restored, dict) and isinstance(restored.get("runs"), list):
                    state = {"runs": restored["runs"][-16:]}
                result = {
                    "protocolVersion": 1,
                    "name": "agents",
                    "tools": [],
                    "commands": [
                        {
                            "name": "subagents",
                            "description": "Run one explicit ephemeral agent",
                            "permissions": ["agent", "persist", "ui"],
                        },
                        {
                            "name": "agents",
                            "description": "Run or inspect an explicit agent batch",
                            "permissions": ["agent", "persist", "ui"],
                        },
                    ],
                    "events": [],
                }
            elif method == "command/run":
                command = params["name"]
                argument = params["arguments"].strip()
                if command == "agents" and not argument:
                    lines = [
                        f"- {run['prompt']}: {run['status']}"
                        for run in state["runs"][-16:]
                    ]
                    result = output(
                        panel={
                            "title": "Explicit agents",
                            "text": "\n".join(lines) or "No explicit agents have run.",
                        }
                    )
                else:
                    prompts = [argument] if command == "subagents" else json.loads(argument)
                    if (
                        not isinstance(prompts, list)
                        or not 1 <= len(prompts) <= 8
                        or not all(isinstance(prompt, str) and 0 < len(prompt) <= MAX_TEXT_BYTES for prompt in prompts)
                    ):
                        raise ValueError("use /subagents <prompt> or /agents [\"prompt\", ...]")
                    pending = prompts
                    results = []
                    result = output(
                        actions=[
                            {"type": "agent", "id": f"agent-{index}", "prompt": prompt, "model": None}
                            for index, prompt in enumerate(prompts)
                        ],
                        status=f"Running {len(prompts)} explicit agent(s).",
                    )
            elif method == "event/notify" and params["name"] == "actionResult":
                if not pending:
                    raise RuntimeError("no explicit agent batch is pending")
                results.append(params["data"])
                if len(results) < len(pending):
                    result = None
                else:
                    runs = []
                    panels = []
                    for prompt, action_result in zip(pending, results):
                        succeeded = action_result.get("succeeded") is True
                        text = str(action_result.get("data", {}).get("text", ""))[:MAX_TEXT_BYTES]
                        runs.append(
                            {
                                "prompt": prompt[:256],
                                "status": "completed" if succeeded else "failed",
                            }
                        )
                        panels.append(f"## {prompt[:256]}\n\n{text}")
                    state["runs"] = (state["runs"] + runs)[-16:]
                    pending = []
                    results = []
                    result = output(
                        records=[state],
                        panel={"title": "Explicit agent results", "text": "\n\n".join(panels)[:MAX_TEXT_BYTES]},
                    )
            elif method == "shutdown":
                respond(request, None)
                return
            else:
                raise RuntimeError("unsupported extension method")
            respond(request, result)
        except Exception as error:
            respond(request, error=error)


if __name__ == "__main__":
    main()
