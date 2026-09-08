#!/usr/bin/env python3
import json
import pathlib
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


def inspect(target):
    return {
        "text": "",
        "records": [],
        "actions": [{"type": "inspect", "id": "inspect", "target": target}],
    }


def main():
    cwd = None
    pending = None
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
                cwd = pathlib.Path(params["cwd"]).resolve()
                result = {
                    "protocolVersion": 1,
                    "name": "diagnostics",
                    "tools": [],
                    "commands": [
                        {
                            "name": "context",
                            "description": "Show bounded model-visible context",
                            "permissions": ["contextRead", "ui", "workspaceWrite"],
                        },
                        {
                            "name": "system-prompt",
                            "description": "Show the complete Antex system prompt",
                            "permissions": ["contextRead", "ui", "workspaceWrite"],
                        },
                        {
                            "name": "dump",
                            "description": "Export the bounded session transcript",
                            "permissions": ["sessionRead", "ui", "workspaceWrite"],
                        },
                    ],
                    "events": [],
                }
            elif method == "command/run":
                command = params["name"]
                argument = params["arguments"].strip()
                pending = (command, argument)
                target = {
                    "context": "context",
                    "system-prompt": "systemPrompt",
                    "dump": "transcript",
                }.get(command)
                if target is None:
                    raise ValueError("unknown diagnostic command")
                result = inspect(target)
            elif method == "event/notify" and params["name"] == "actionResult":
                if pending is None:
                    raise RuntimeError("no inspection is pending")
                if not params["data"].get("succeeded"):
                    raise RuntimeError(params["data"].get("data", {}).get("text", "inspection failed"))
                text = str(params["data"].get("data", {}).get("text", ""))[:MAX_TEXT_BYTES]
                command, argument = pending
                pending = None
                if command == "dump":
                    relative = pathlib.PurePosixPath(argument or "antex-transcript.md")
                    if relative.is_absolute() or ".." in relative.parts:
                        raise ValueError("dump path must stay inside the workspace")
                    path = (cwd / relative).resolve()
                    try:
                        path.relative_to(cwd)
                    except ValueError:
                        raise ValueError("dump path must stay inside the workspace")
                    path.parent.mkdir(parents=True, exist_ok=True)
                    with path.open("x", encoding="utf-8") as output:
                        output.write(text)
                    result = {"text": f"Exported transcript to {relative}", "records": [], "actions": []}
                else:
                    result = {
                        "text": "",
                        "records": [],
                        "actions": [],
                        "panel": {
                            "title": "Model context" if command == "context" else "System prompt",
                            "text": text,
                        },
                    }
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
