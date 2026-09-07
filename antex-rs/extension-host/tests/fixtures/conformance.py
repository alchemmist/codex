import json
import os
import sys
import time


def respond(request, result=None, error=None):
    response = {"jsonrpc": "2.0", "id": request["id"]}
    if error is None:
        response["result"] = result
    else:
        response["error"] = error
    print(json.dumps(response, separators=(",", ":")), flush=True)


state = None
for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    if method == "initialize":
        state = request["params"].get("state")
        permissions = request["params"]["capabilities"]
        respond(
            request,
            {
                "protocolVersion": 1,
                "name": "fixture",
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo text",
                        "parameters": {
                            "type": "object",
                            "properties": {"text": {"type": "string"}},
                            "required": ["text"],
                            "additionalProperties": False,
                        },
                        "permissions": permissions,
                    }
                ],
                "commands": [
                    {"name": "hello", "description": "Say hello", "permissions": permissions}
                ],
                "events": ["turnComplete"],
            },
        )
    elif method == "tool/call":
        arguments = request["params"]["arguments"]
        if arguments.get("text") == "stall":
            time.sleep(60)
        if arguments.get("text") == "crash":
            os._exit(9)
        if arguments.get("text") == "malformed":
            print("not-json", flush=True)
            continue
        if arguments.get("text") == "oversized":
            print("x" * (256 * 1024 + 1), flush=True)
            continue
        actions = []
        if arguments.get("text") == "agent":
            actions = [
                {"type": "agent", "id": "worker", "prompt": "work", "model": None}
            ]
        respond(request, {"text": arguments["text"], "records": [], "actions": actions})
    elif method == "command/run":
        respond(request, {"text": "hello", "records": [], "actions": []})
    elif method == "event/notify":
        if request["params"]["data"].get("fail"):
            respond(request, error={"code": -32000, "message": "event failed"})
        elif request["params"]["data"].get("terminalLog"):
            respond(
                request,
                {
                    "text": "",
                    "records": [],
                    "actions": [
                        {"type": "terminalLog", "id": "log", "text": "command"}
                    ],
                },
            )
        elif request["params"]["data"].get("shell"):
            respond(
                request,
                {
                    "text": "",
                    "records": [],
                    "actions": [
                        {
                            "type": "shell",
                            "id": "shell",
                            "command": "pwd",
                            "workspace": "readOnly",
                            "network": "denied",
                        }
                    ],
                },
            )
        elif request["params"]["data"].get("state"):
            respond(
                request,
                {"text": json.dumps(state), "records": [], "actions": []},
            )
        else:
            respond(request, None)
    elif method == "shutdown":
        respond(request, None)
        break
