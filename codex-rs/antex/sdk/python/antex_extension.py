import json
import sys


PROTOCOL_VERSION = 1


class Extension:
    def __init__(self, name):
        self.name = name
        self.tools = {}
        self.commands = {}
        self.events = set()

    def tool(self, name, description, parameters, permissions=()):
        def register(handler):
            self.tools[name] = (description, parameters, list(permissions), handler)
            return handler

        return register

    def command(self, name, description, permissions=()):
        def register(handler):
            self.commands[name] = (description, list(permissions), handler)
            return handler

        return register

    def observe(self, *events):
        self.events.update(events)

    def run(self):
        for line in sys.stdin:
            request = json.loads(line)
            try:
                result = self._dispatch(request["method"], request.get("params", {}))
                response = {"jsonrpc": "2.0", "id": request["id"], "result": result}
            except Exception as error:
                response = {
                    "jsonrpc": "2.0",
                    "id": request.get("id", ""),
                    "error": {"code": -32000, "message": str(error)[:512]},
                }
            print(json.dumps(response, separators=(",", ":")), flush=True)
            if request.get("method") == "shutdown":
                return

    def _dispatch(self, method, params):
        if method == "initialize":
            if params["protocolVersion"] != PROTOCOL_VERSION:
                raise ValueError("incompatible protocol version")
            return {
                "protocolVersion": PROTOCOL_VERSION,
                "name": self.name,
                "tools": [
                    {
                        "name": name,
                        "description": value[0],
                        "parameters": value[1],
                        "permissions": value[2],
                    }
                    for name, value in self.tools.items()
                ],
                "commands": [
                    {
                        "name": name,
                        "description": value[0],
                        "permissions": value[1],
                    }
                    for name, value in self.commands.items()
                ],
                "events": sorted(self.events),
            }
        if method == "tool/call":
            return self.tools[params["name"]][3](params["arguments"])
        if method == "command/run":
            return self.commands[params["name"]][2](params["arguments"])
        if method == "event/notify":
            handler = getattr(self, "on_event", None)
            return handler(params) if handler else None
        if method == "shutdown":
            return None
        raise ValueError("unknown method")


def output(text="", records=None, status=None, panel=None, actions=None):
    return {
        "text": text,
        "records": records or [],
        "status": status,
        "panel": panel,
        "actions": actions or [],
    }
