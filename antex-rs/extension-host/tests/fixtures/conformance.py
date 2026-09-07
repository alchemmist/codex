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


for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    if method == "initialize":
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
                        "permissions": [],
                    }
                ],
                "commands": [
                    {"name": "hello", "description": "Say hello", "permissions": []}
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
        else:
            respond(request, None)
    elif method == "shutdown":
        respond(request, None)
        break
