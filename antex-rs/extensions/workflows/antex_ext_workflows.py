#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import queue
import re
import shlex
import sys
import threading


MAX_FRAME_BYTES = 256 * 1024
MAX_TEXT_BYTES = 8000
ID = re.compile(r"^[A-Za-z0-9_-]{1,64}$")


def respond(request, result=None, error=None):
    message = {"jsonrpc": "2.0", "id": request.get("id", "")}
    if error is None:
        message["result"] = result
    else:
        message["error"] = {"code": -32000, "message": str(error)[:512]}
    print(json.dumps(message, separators=(",", ":")), flush=True)


class Context:
    def __init__(self, params, state, pending, results):
        self.params = params
        self.state = state
        self.pending = pending
        self.results = results
        self.status = None

    def action(self, action):
        self.pending.put(("action", action, self.snapshot()))
        result = self.results.get()
        if not result.get("succeeded"):
            raise RuntimeError(str(result.get("data", {}).get("text", "action failed")))
        return result.get("data", {})

    def shell(self, argv, cwd=None, timeout_seconds=None, env=None):
        if not isinstance(argv, list) or not argv or not all(isinstance(item, str) for item in argv):
            raise ValueError("ctx.shell argv must be a non-empty string list")
        if timeout_seconds is not None or env is not None:
            raise ValueError("per-step timeout and environment overrides are unsupported")
        command = shlex.join(argv)
        if cwd is not None:
            relative = pathlib.PurePosixPath(cwd)
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("workflow cwd must stay inside the workspace")
            command = f"cd {shlex.quote(str(relative))} && {command}"
        return self.action(
            {
                "type": "shell",
                "id": "shell",
                "command": command,
                "workspace": "readWrite",
                "network": "denied",
            }
        )

    def agent(self, prompt, model=None, **options):
        unsupported = set(options) - {"reasoning_effort", "developer_instructions", "cwd"}
        if unsupported:
            raise ValueError(f"unsupported agent options: {sorted(unsupported)}")
        if not isinstance(prompt, str) or not prompt or len(prompt) > MAX_TEXT_BYTES:
            raise ValueError("agent prompt is empty or exceeds its budget")
        return self.action(
            {"type": "agent", "id": "agent", "prompt": prompt, "model": model}
        )

    def agent_batch(self, prompts, parallelism=None, **options):
        if parallelism is not None and not 1 <= int(parallelism) <= 8:
            raise ValueError("parallelism must be between 1 and 8")
        return [self.agent(prompt, **options) for prompt in prompts]

    def inspect(self, target):
        if target not in {"transcript", "context", "systemPrompt"}:
            raise ValueError("unknown inspection target")
        return self.action({"type": "inspect", "id": "inspect", "target": target})

    def checkpoint(self, state):
        encoded = json.dumps(state, separators=(",", ":"))
        if len(encoded.encode()) > MAX_TEXT_BYTES:
            raise ValueError("workflow checkpoint exceeds its budget")
        self.state = state

    def progress(self, message, current=None, total=None):
        text = str(message)
        if current is not None and total is not None:
            text = f"{text} ({current}/{total})"
        self.status = text[:256].replace("\n", " ").replace("\r", " ")

    def log(self, message):
        print(str(message)[:MAX_TEXT_BYTES], file=sys.stderr, flush=True)

    def snapshot(self):
        return {"state": self.state, "status": self.status}


class Runner:
    def __init__(self, cwd, restored):
        self.cwd = pathlib.Path(cwd).resolve()
        self.restored = restored if isinstance(restored, dict) else {}
        self.pending = queue.Queue(maxsize=1)
        self.results = queue.Queue(maxsize=1)
        self.thread = None

    def start(self, arguments):
        if self.thread is not None and self.thread.is_alive():
            raise RuntimeError("a workflow is already running")
        workflow_id, separator, encoded = arguments.strip().partition(" ")
        if not ID.fullmatch(workflow_id):
            raise ValueError("usage: /workflow <id> [json-params]")
        params = json.loads(encoded) if separator else {}
        if not isinstance(params, dict):
            raise ValueError("workflow parameters must be a JSON object")
        root = (self.cwd / ".antex" / "workflows").resolve()
        path = (root / f"{workflow_id}.py").resolve()
        try:
            path.relative_to(root)
        except ValueError:
            raise ValueError(f"workflow not found: {workflow_id}")
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"workflow not found: {workflow_id}")
        spec = importlib.util.spec_from_file_location(f"antex_workflow_{workflow_id}", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        manifest = getattr(module, "WORKFLOW", {})
        if manifest.get("id") != workflow_id or not callable(getattr(module, "run", None)):
            raise ValueError("workflow manifest or run function is invalid")
        restored = self.restored if self.restored.get("workflow") == workflow_id else {}
        context = Context(params, restored.get("state", {}), self.pending, self.results)

        def run():
            try:
                result = module.run(context)
                self.pending.put(("done", result, context.snapshot()))
            except Exception as error:
                self.pending.put(("error", str(error), context.snapshot()))

        self.thread = threading.Thread(target=run, name="antex-workflow", daemon=True)
        self.thread.start()
        return self.next(workflow_id)

    def resume(self, workflow_id, result):
        if self.thread is None or not self.thread.is_alive():
            raise RuntimeError("no workflow action is awaiting a result")
        self.results.put(result)
        return self.next(workflow_id)

    def next(self, workflow_id):
        kind, value, snapshot = self.pending.get(timeout=900)
        record = {"workflow": workflow_id, "state": snapshot["state"]}
        common = {"records": [record], "actions": []}
        if snapshot["status"] is not None:
            common["status"] = snapshot["status"]
        if kind == "action":
            common.update({"text": "", "actions": [value]})
            return common
        if kind == "done":
            common["text"] = json.dumps(value, ensure_ascii=False)[:MAX_TEXT_BYTES]
            return common
        raise RuntimeError(value)


def main():
    runner = None
    workflow_id = None
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
                runner = Runner(params["cwd"], params.get("state"))
                result = {
                    "protocolVersion": 1,
                    "name": "workflows",
                    "tools": [],
                    "commands": [
                        {
                            "name": "workflow",
                            "description": "Run a project Python workflow",
                            "permissions": [
                                "agent",
                                "contextRead",
                                "persist",
                                "sessionRead",
                                "shell",
                                "ui",
                                "workspaceRead",
                                "workspaceWrite",
                            ],
                        }
                    ],
                    "events": [],
                }
            elif method == "command/run":
                workflow_id = params["arguments"].strip().partition(" ")[0]
                result = runner.start(params["arguments"])
            elif method == "event/notify" and params["name"] == "actionResult":
                result = runner.resume(workflow_id, params["data"])
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
