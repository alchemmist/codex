#!/usr/bin/env python3
import json
import sys


MAX_FRAME_BYTES = 256 * 1024


def respond(request, result=None, error=None):
    message = {"jsonrpc": "2.0", "id": request.get("id", "")}
    if error is None:
        message["result"] = result
    else:
        message["error"] = {"code": -32000, "message": str(error)[:512]}
    print(json.dumps(message, separators=(",", ":")), flush=True)


def output(text="", records=None, actions=None, status=None):
    value = {
        "text": text,
        "records": records or [],
        "actions": actions or [],
    }
    if status is not None:
        value["status"] = status
    return value


def main():
    enabled = False
    commands = {}
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
                state = params.get("state")
                enabled = isinstance(state, dict) and state.get("enabled") is True
                result = {
                    "protocolVersion": 1,
                    "name": "tmux-log",
                    "tools": [],
                    "commands": [
                        {
                            "name": "tmux-command-log",
                            "description": "Toggle the dedicated tmux command log",
                            "permissions": ["persist", "ui"],
                        }
                    ],
                    "events": ["toolStarted", "toolCompleted"],
                }
            elif method == "command/run":
                if params["name"] != "tmux-command-log":
                    raise RuntimeError("unknown command")
                enabled = not enabled
                state = {"enabled": enabled}
                result = output(
                    records=[state],
                    status=f"Tmux command log {'enabled' if enabled else 'disabled'}.",
                )
            elif method == "event/notify":
                if not enabled:
                    result = None
                elif params["name"] == "toolStarted" and params["data"].get("name") == "shell":
                    call_id = str(params["data"].get("callId", ""))[:128]
                    command = str(params["data"].get("arguments", {}).get("command", ""))
                    commands[call_id] = command
                    result = output(
                        actions=[
                            {
                                "type": "terminalLog",
                                "id": f"start-{call_id}"[:64],
                                "text": f"$ {command}"[:8000],
                            }
                        ]
                    )
                elif params["name"] == "toolCompleted":
                    call_id = str(params["data"].get("callId", ""))[:128]
                    if call_id not in commands:
                        result = None
                    else:
                        commands.pop(call_id, None)
                        text = str(params["data"].get("text", ""))[:7800]
                        outcome = str(params["data"].get("outcome", "unknown"))
                        result = output(
                            actions=[
                                {
                                    "type": "terminalLog",
                                    "id": f"finish-{call_id}"[:64],
                                    "text": f"{text}\n[{outcome}]"[:8000],
                                }
                            ]
                        )
                else:
                    result = None
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
