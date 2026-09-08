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


def render(items):
    return "\n".join(f"- [{'x' if item['done'] else ' '}] {item['text']}" for item in items) or "No TODO items."


def main():
    state = {"items": []}
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
                if isinstance(restored, dict) and isinstance(restored.get("items"), list):
                    state = {"items": restored["items"][:64]}
                result = {
                    "protocolVersion": 1,
                    "name": "plan",
                    "tools": [],
                    "commands": [
                        {"name": "todo", "description": "Show or update the persistent TODO panel", "permissions": ["persist", "ui"]},
                        {"name": "todo-clear", "description": "Clear the persistent TODO panel", "permissions": ["persist", "ui"]},
                    ],
                    "events": [],
                }
            elif method == "command/run":
                command = params["name"]
                argument = params["arguments"].strip()
                changed = False
                if command == "todo-clear":
                    state = {"items": []}
                    changed = True
                elif command == "todo" and argument:
                    if argument.startswith("["):
                        values = json.loads(argument)
                        if not isinstance(values, list):
                            raise ValueError("TODO JSON must be an array")
                        items = []
                        for value in values[:64]:
                            if isinstance(value, str):
                                items.append({"text": value[:512], "done": False})
                            elif isinstance(value, dict) and isinstance(value.get("text"), str):
                                items.append({"text": value["text"][:512], "done": value.get("done") is True})
                            else:
                                raise ValueError("invalid TODO item")
                        state = {"items": items}
                    else:
                        if len(state["items"]) >= 64:
                            raise ValueError("TODO list reached 64 items")
                        state["items"].append({"text": argument[:512], "done": False})
                    changed = True
                elif command != "todo":
                    raise ValueError("unknown plan command")
                result = {
                    "text": "",
                    "records": [state] if changed else [],
                    "actions": [],
                    "panel": {"title": "TODO", "text": render(state["items"])},
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
