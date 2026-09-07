#!/usr/bin/env python3
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request


PROTOCOL_VERSION = 1
MAX_FRAME_BYTES = 256 * 1024
MAX_TEXT_BYTES = 8000


class Mcp:
    def __init__(self, command):
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=sys.stderr,
            text=True,
            bufsize=1,
        )
        self.next_id = 1

    def request(self, method, params):
        request_id = self.next_id
        self.next_id += 1
        self.write(
            {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        )
        while True:
            response = self.read()
            if "method" in response and "id" in response:
                self.write(
                    {
                        "jsonrpc": "2.0",
                        "id": response["id"],
                        "error": {
                            "code": -32601,
                            "message": "MCP server requests are unsupported",
                        },
                    }
                )
                continue
            if response.get("id") != request_id:
                continue
            if "error" in response:
                error = response["error"]
                raise RuntimeError(
                    str(error.get("message", "MCP request failed"))[:512]
                )
            return response.get("result")

    def notify(self, method, params=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.write(message)

    def write(self, message):
        frame = json.dumps(message, separators=(",", ":"))
        if len(frame.encode()) > MAX_FRAME_BYTES:
            raise RuntimeError("MCP request exceeds its frame budget")
        self.process.stdin.write(frame + "\n")
        self.process.stdin.flush()

    def read(self):
        line = self.process.stdout.readline(MAX_FRAME_BYTES + 2)
        if not line:
            raise RuntimeError("MCP server exited")
        if len(line.encode()) > MAX_FRAME_BYTES or not line.endswith("\n"):
            raise RuntimeError("MCP response exceeds its frame budget")
        message = json.loads(line)
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            raise RuntimeError("invalid MCP response")
        return message

    def initialize(self):
        self.request(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "antex-ext-mcp", "version": "0.0.0"},
            },
        )
        self.notify("notifications/initialized")

    def tools(self):
        tools = []
        cursor = None
        for _ in range(16):
            params = {} if cursor is None else {"cursor": cursor}
            result = self.request("tools/list", params)
            page = result.get("tools", [])
            if not isinstance(page, list):
                raise RuntimeError("invalid MCP tool catalog")
            tools.extend(page)
            if len(tools) > 32:
                raise RuntimeError("MCP tool catalog exceeds 32 tools")
            cursor = result.get("nextCursor")
            if cursor is None:
                return tools
        raise RuntimeError("MCP tool catalog pagination exceeded its limit")

    def call(self, name, arguments):
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        text = []
        for content in result.get("content", [])[:64]:
            if content.get("type") == "text":
                text.append(str(content.get("text", "")))
            elif content.get("type") == "image":
                text.append("[MCP image result omitted]")
            elif content.get("type") in ("resource", "resource_link"):
                text.append(json.dumps(content, separators=(",", ":")))
        output = "\n".join(text)
        encoded = output.encode()
        if len(encoded) > MAX_TEXT_BYTES:
            output = (
                encoded[: MAX_TEXT_BYTES - 20].decode("utf-8", "ignore")
                + "\n[output truncated]"
            )
        if result.get("isError"):
            raise RuntimeError(output or "MCP tool failed")
        return output

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()


class HttpMcp:
    initialize = Mcp.initialize
    tools = Mcp.tools
    call = Mcp.call

    def __init__(self, url, token_file=None):
        parsed = urllib.parse.urlparse(url)
        if parsed.scheme != "https" and not (
            parsed.scheme == "http" and parsed.hostname in {"127.0.0.1", "localhost", "::1"}
        ):
            raise ValueError("HTTP MCP URL must use HTTPS or loopback HTTP")
        if parsed.username or parsed.password or parsed.fragment:
            raise ValueError("HTTP MCP URL contains forbidden credentials or fragment")
        self.url = url
        self.next_id = 1
        self.session_id = None
        self.token = None
        if token_file is not None:
            path = pathlib.Path(token_file)
            if not path.is_absolute():
                path = pathlib.Path(__file__).resolve().parent / path
            if path.is_symlink() or not path.is_file() or path.stat().st_size > 16 * 1024:
                raise ValueError("invalid MCP bearer token file")
            self.token = path.read_text(encoding="utf-8").strip()
            if not self.token or len(self.token) > 16 * 1024 or any(character.isspace() for character in self.token):
                raise ValueError("invalid MCP bearer token")

    def request(self, method, params):
        request_id = self.next_id
        self.next_id += 1
        messages = self.post(
            {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        )
        for message in messages:
            if "method" in message and "id" in message:
                self.post(
                    {
                        "jsonrpc": "2.0",
                        "id": message["id"],
                        "error": {
                            "code": -32601,
                            "message": "MCP server requests are unsupported",
                        },
                    }
                )
                continue
            if message.get("id") != request_id:
                continue
            if "error" in message:
                raise RuntimeError(str(message["error"].get("message", "MCP request failed"))[:512])
            return message.get("result")
        raise RuntimeError("HTTP MCP response did not contain the matching JSON-RPC result")

    def notify(self, method, params=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.post(message)

    def post(self, message):
        body = json.dumps(message, separators=(",", ":")).encode()
        if len(body) > MAX_FRAME_BYTES:
            raise RuntimeError("MCP request exceeds its frame budget")
        headers = {
            "Accept": "application/json, text/event-stream",
            "Content-Type": "application/json",
        }
        if self.session_id is not None:
            headers["Mcp-Session-Id"] = self.session_id
            headers["MCP-Protocol-Version"] = "2025-06-18"
        if self.token is not None:
            headers["Authorization"] = f"Bearer {self.token}"
        request = urllib.request.Request(self.url, data=body, headers=headers, method="POST")
        try:
            response = urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as error:
            if error.code == 401:
                challenge = error.headers.get("WWW-Authenticate", "")[:512]
                raise RuntimeError(f"HTTP MCP authorization required: {challenge}")
            raise RuntimeError(f"HTTP MCP request failed with status {error.code}")
        with response:
            session_id = response.headers.get("Mcp-Session-Id")
            if session_id is not None:
                if not session_id or len(session_id) > 1024 or any(not 0x21 <= ord(character) <= 0x7E for character in session_id):
                    raise RuntimeError("HTTP MCP returned an invalid session identifier")
                self.session_id = session_id
            if response.status == 202:
                return []
            content_type = response.headers.get_content_type()
            if content_type == "application/json":
                data = response.read(MAX_FRAME_BYTES + 1)
                if len(data) > MAX_FRAME_BYTES:
                    raise RuntimeError("HTTP MCP response exceeds its frame budget")
                return [self.validate(json.loads(data))]
            if content_type == "text/event-stream":
                return self.read_sse(response)
            raise RuntimeError(f"HTTP MCP returned unsupported content type {content_type}")

    def read_sse(self, response):
        messages = []
        data = []
        total = 0
        while True:
            line = response.readline(MAX_FRAME_BYTES + 2)
            if not line:
                break
            total += len(line)
            if len(line) > MAX_FRAME_BYTES or total > 8 * MAX_FRAME_BYTES:
                raise RuntimeError("HTTP MCP SSE response exceeds its budget")
            line = line.decode("utf-8").rstrip("\r\n")
            if not line:
                if data:
                    messages.append(self.validate(json.loads("\n".join(data))))
                    data = []
                continue
            if line.startswith("data:"):
                data.append(line[5:].lstrip())
        if data:
            messages.append(self.validate(json.loads("\n".join(data))))
        return messages

    def validate(self, message):
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            raise RuntimeError("invalid MCP response")
        return message

    def close(self):
        pass


def response(request, result=None, error=None):
    message = {"jsonrpc": "2.0", "id": request.get("id", "")}
    if error is None:
        message["result"] = result
    else:
        message["error"] = {"code": -32000, "message": str(error)[:512]}
    frame = json.dumps(message, separators=(",", ":"))
    if len(frame.encode()) > MAX_FRAME_BYTES:
        frame = json.dumps(
            {
                "jsonrpc": "2.0",
                "id": request.get("id", ""),
                "error": {
                    "code": -32001,
                    "message": "extension response exceeds its budget",
                },
            },
            separators=(",", ":"),
        )
    print(frame, flush=True)


def tool_alias(name):
    normalized = "".join(
        character if character.isascii() and character.isalnum() else "_"
        for character in name
    )
    normalized = normalized.strip("_") or "tool"
    digest = hashlib.sha256(name.encode()).hexdigest()[:8]
    return f"{normalized[:55]}_{digest}"


def manifest(name, tools, permissions):
    definitions = []
    aliases = {}
    for tool in tools:
        tool_name = tool.get("name")
        schema = tool.get("inputSchema", {"type": "object"})
        if not isinstance(tool_name, str) or not isinstance(schema, dict):
            raise RuntimeError("invalid MCP tool definition")
        alias = tool_alias(tool_name)
        if alias in aliases:
            raise RuntimeError("MCP tool aliases collide")
        aliases[alias] = tool_name
        definitions.append(
            {
                "name": alias,
                "description": str(tool.get("description", ""))[:MAX_TEXT_BYTES],
                "parameters": schema,
                "permissions": permissions,
            }
        )
    return (
        {
            "protocolVersion": PROTOCOL_VERSION,
            "name": name,
            "tools": definitions,
            "commands": [
                {
                    "name": "status",
                    "description": "Show MCP server status",
                    "permissions": permissions,
                }
            ],
            "events": [],
        },
        aliases,
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--name", required=True)
    parser.add_argument("--url")
    parser.add_argument("--bearer-token-file")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if bool(args.url) == bool(command):
        raise SystemExit("provide exactly one MCP URL or stdio server command")
    if args.bearer_token_file and not args.url:
        raise SystemExit("--bearer-token-file requires --url")
    mcp = HttpMcp(args.url, args.bearer_token_file) if args.url else Mcp(command)
    tools = None
    aliases = {}
    try:
        for line in sys.stdin:
            if len(line.encode()) > MAX_FRAME_BYTES:
                raise RuntimeError("extension request exceeds its frame budget")
            request = json.loads(line)
            try:
                method = request["method"]
                if method == "initialize":
                    if request["params"]["protocolVersion"] != PROTOCOL_VERSION:
                        raise RuntimeError("incompatible extension protocol version")
                    mcp.initialize()
                    tools = mcp.tools()
                    permissions = request["params"]["capabilities"]
                    result, aliases = manifest(args.name, tools, permissions)
                elif method == "tool/call":
                    result = {
                        "text": mcp.call(
                            aliases[request["params"]["name"]],
                            request["params"]["arguments"],
                        ),
                        "records": [],
                        "actions": [],
                    }
                elif method == "command/run":
                    result = {
                        "text": f"{args.name}: {len(tools or [])} tools",
                        "records": [],
                        "actions": [],
                    }
                elif method == "event/notify":
                    result = None
                elif method == "shutdown":
                    response(request, None)
                    return
                else:
                    raise RuntimeError("unknown extension method")
                response(request, result)
            except Exception as error:
                response(request, error=error)
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
