#!/usr/bin/env python3
import hashlib
import json
import os
import pathlib
import queue
import re
import shlex
import sys
import threading
import time
import types
import uuid


MAX_FRAME_BYTES = 256 * 1024
MAX_TEXT_BYTES = 8000
ID = re.compile(r"^[A-Za-z0-9_-]{1,64}$")


def encoded_size(value):
    return len(json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode())


def workflow_paths(cwd):
    roots = []
    personal = os.environ.get("ANTEX_WORKFLOWS_DIR")
    if personal:
        roots.append(pathlib.Path(personal))
    if os.environ.get("ANTEX_TRUST_PROJECT_WORKFLOWS") == "1":
        roots.append(pathlib.Path(cwd) / ".antex" / "workflows")
    paths = {}
    for root in roots:
        if root.is_symlink() or not root.is_dir():
            continue
        root = root.resolve()
        for path in sorted(root.glob("*.py")):
            if path.is_symlink() or not path.is_file() or not ID.fullmatch(path.stem):
                continue
            if path.stem in paths:
                raise ValueError(f"duplicate workflow id: {path.stem}")
            paths[path.stem] = path
            if len(paths) > 256:
                raise ValueError("workflow catalog exceeds 256 entries")
    return paths


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

    def action_batch(self, actions):
        self.pending.put(("actions", actions, self.snapshot()))
        results = [self.results.get() for _ in actions]
        for result in results:
            if not result.get("succeeded"):
                raise RuntimeError(str(result.get("data", {}).get("text", "action failed")))
        return [result.get("data", {}) for result in results]

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
        unsupported = set(options)
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
        prompts = list(prompts)
        if len(prompts) > 64:
            raise ValueError("agent batch exceeds 64 prompts")
        actions = []
        for index, prompt in enumerate(prompts):
            if isinstance(prompt, dict):
                prompt_options = dict(options)
                prompt_options.update(prompt)
                text = prompt_options.pop("prompt")
                model = prompt_options.pop("model", None)
            else:
                text = prompt
                model = None
                prompt_options = options
            unsupported = set(prompt_options)
            if unsupported:
                raise ValueError(f"unsupported agent options: {sorted(unsupported)}")
            if not isinstance(text, str) or not text or len(text) > MAX_TEXT_BYTES:
                raise ValueError("agent prompt is empty or exceeds its budget")
            actions.append(
                {
                    "type": "agent",
                    "id": f"agent-{index}",
                    "prompt": text,
                    "model": model,
                }
            )
        results = []
        width = int(parallelism) if parallelism is not None else 8
        for offset in range(0, len(actions), width):
            results.extend(self.action_batch(actions[offset:offset + width]))
        return results

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
        self.restored = dict(restored) if isinstance(restored, dict) else {}
        if self.restored.get("phase") == "running":
            self.restored["phase"] = "interrupted"
        self.pending = queue.Queue(maxsize=1)
        self.results = queue.Queue(maxsize=1)
        self.thread = None
        self.awaiting = 0
        self.metadata = None

    def start(self, arguments):
        if arguments.strip() == "status":
            return {"text": self.restored.get("phase", "No workflow run recorded."),
                    "records": [self.restored] if self.restored else [], "actions": []}
        if self.thread is not None and self.thread.is_alive():
            raise RuntimeError("a workflow is already running")
        workflow_id, separator, encoded = arguments.strip().partition(" ")
        resuming = workflow_id == "resume"
        if resuming:
            if separator or self.restored.get("phase") not in {"failed", "interrupted", "stopped"}:
                raise ValueError("no interrupted workflow to resume")
            workflow_id = self.restored.get("workflow", "")
        if not workflow_id or workflow_id == "list":
            return {"text": "\n".join(workflow_paths(self.cwd)) or "No trusted workflows found.", "records": [], "actions": []}
        if not ID.fullmatch(workflow_id):
            raise ValueError("usage: /workflow <id> [json-params]")
        params = self.restored.get("params", {}) if resuming else json.loads(encoded) if separator else {}
        if not isinstance(params, dict):
            raise ValueError("workflow parameters must be a JSON object")
        path = workflow_paths(self.cwd).get(workflow_id)
        if path is None:
            raise ValueError(f"workflow not found: {workflow_id}")
        if resuming:
            source_bytes = self.restored.get("source", "").encode("utf-8")
            if hashlib.sha256(source_bytes).hexdigest() != self.restored.get("sourceSha256"):
                raise ValueError("workflow source snapshot hash mismatch")
        else:
            with path.open("rb") as source_file:
                source_bytes = source_file.read(4097)
        if len(source_bytes) > 4096:
            raise ValueError("workflow source exceeds its 4096-byte snapshot budget")
        source = source_bytes.decode("utf-8")
        self.metadata = {"workflow": workflow_id, "runId": uuid.uuid4().hex,
                         "source": source, "sourceSha256": hashlib.sha256(source_bytes).hexdigest(),
                         "params": params, "startedAt": int(time.time()), "phase": "running"}
        if resuming:
            self.metadata.update({"runId": self.restored["runId"],
                                  "startedAt": self.restored["startedAt"], "resumedAt": int(time.time())})
        if encoded_size(self.metadata) > 6000:
            raise ValueError("workflow source and parameters exceed their record budget")
        module = types.ModuleType(f"antex_workflow_{workflow_id}")
        module.__file__ = str(path)
        self.pending = queue.Queue(maxsize=1)
        self.results = queue.Queue(maxsize=1)
        context = Context(params, self.restored.get("state", {}) if resuming else {}, self.pending, self.results)

        def run():
            try:
                exec(compile(source, str(path), "exec"), module.__dict__)
                manifest = getattr(module, "WORKFLOW", {})
                if manifest.get("id") != workflow_id or not callable(getattr(module, "run", None)):
                    raise ValueError("workflow manifest or run function is invalid")
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
        self.awaiting -= 1
        if self.awaiting > 0:
            return None
        return self.next(workflow_id)

    def next(self, workflow_id):
        kind, value, snapshot = self.pending.get(timeout=900)
        record = {**self.metadata, "state": snapshot["state"]}
        record["phase"] = {"done": "completed", "error": "failed"}.get(kind, "running")
        if kind == "error":
            record["error"] = str(value)[:256]
        if kind in {"done", "error"}:
            record["finishedAt"] = int(time.time())
        if encoded_size(record) > MAX_TEXT_BYTES:
            record["state"] = {}
            record["phase"] = "failed"
            record["error"] = "workflow checkpoint exceeds the remaining record budget"
            kind, value = "error", record["error"]
        self.restored = record
        common = {"records": [record], "actions": []}
        if snapshot["status"] is not None:
            common["status"] = snapshot["status"]
        if kind in {"action", "actions"}:
            actions = [value] if kind == "action" else value
            self.awaiting = len(actions)
            common.update({"text": "", "actions": actions})
            return common
        if kind == "done":
            common["text"] = json.dumps(value, ensure_ascii=False)[:MAX_TEXT_BYTES]
            return common
        common["text"] = f"Workflow failed: {value}".encode()[:MAX_TEXT_BYTES].decode("utf-8", "ignore")
        return common


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
