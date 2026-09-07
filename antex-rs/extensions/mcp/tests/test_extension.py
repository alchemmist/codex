import json
import http.server
import pathlib
import subprocess
import sys
import tempfile
import threading
import unittest


ROOT = pathlib.Path(__file__).parents[1]


class McpHandler(http.server.BaseHTTPRequestHandler):
    requests = []

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.__class__.requests.append((body, dict(self.headers)))
        method = body.get("method")
        if method == "notifications/initialized":
            self.send_response(202)
            self.end_headers()
            return
        if method == "initialize":
            result = {"protocolVersion": "2025-06-18", "capabilities": {}, "serverInfo": {"name": "fake", "version": "1"}}
            content_type = "application/json"
        elif method == "tools/list":
            result = {"tools": [{"name": "echo", "description": "Echo", "inputSchema": {"type": "object"}}]}
            content_type = "text/event-stream"
        elif method == "tools/call":
            result = {"content": [{"type": "text", "text": body["params"]["arguments"]["text"]}]}
            content_type = "application/json"
        else:
            self.send_response(400)
            self.end_headers()
            return
        message = {"jsonrpc": "2.0", "id": body["id"], "result": result}
        encoded = json.dumps(message, separators=(",", ":")).encode()
        if content_type == "text/event-stream":
            encoded = b"data: " + encoded + b"\n\n"
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        if method == "initialize":
            self.send_header("Mcp-Session-Id", "session-1")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, *_args):
        pass


class ExtensionTest(unittest.TestCase):
    def test_streamable_http_supports_json_sse_sessions_and_bearer_tokens(self):
        McpHandler.requests = []
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), McpHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        token_directory = tempfile.TemporaryDirectory()
        token = pathlib.Path(token_directory.name, "token")
        token.write_text("secret-token")
        process = subprocess.Popen(
            [
                sys.executable,
                str(ROOT / "antex_ext_mcp.py"),
                "--name",
                "http",
                "--url",
                f"http://127.0.0.1:{server.server_port}/mcp",
                "--bearer-token-file",
                str(token),
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        try:
            initialized = self.request(
                process,
                "1",
                "initialize",
                {"protocolVersion":1,"antexVersion":"0.0.0","sessionId":"test","cwd":"/tmp","capabilities":["network"],"state":None},
            )
            tool_name = initialized["tools"][0]["name"]
            result = self.request(process, "2", "tool/call", {"name":tool_name,"arguments":{"text":"through-http"}})
            self.assertEqual(result["text"], "through-http")
            self.assertEqual(len(McpHandler.requests), 4)
            for _body, headers in McpHandler.requests:
                self.assertEqual(headers["Authorization"], "Bearer secret-token")
            for _body, headers in McpHandler.requests[1:]:
                self.assertEqual(headers["Mcp-Session-Id"], "session-1")
                self.assertEqual(headers["Mcp-Protocol-Version"], "2025-06-18")
        finally:
            process.kill()
            process.wait()
            process.stdin.close()
            process.stdout.close()
            server.shutdown()
            server.server_close()
            token_directory.cleanup()

    def test_stdio_server_tools_are_registered_and_called(self):
        process = subprocess.Popen(
            [
                sys.executable,
                str(ROOT / "antex_ext_mcp.py"),
                "--name",
                "fixture",
                "--",
                sys.executable,
                str(ROOT / "tests/fake_server.py"),
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        try:
            initialized = self.request(
                process,
                "1",
                "initialize",
                {
                    "protocolVersion": 1,
                    "antexVersion": "0.0.0",
                    "sessionId": "test",
                    "cwd": "/tmp",
                    "capabilities": ["shell"],
                    "state": None,
                },
            )
            tool_name = initialized["tools"][0]["name"]
            self.assertTrue(tool_name.startswith("echo_"))
            output = self.request(
                process,
                "2",
                "tool/call",
                {"name": tool_name, "arguments": {"text": "through-mcp"}},
            )
            self.assertEqual(
                output,
                {"text": "through-mcp", "records": [], "actions": []},
            )
            self.request(process, "3", "shutdown", {})
            self.assertEqual(process.wait(timeout=2), 0)
        finally:
            process.kill()
            process.wait()
            process.stdin.close()
            process.stdout.close()

    def request(self, process, request_id, method, params):
        process.stdin.write(
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": method,
                    "params": params,
                },
                separators=(",", ":"),
            )
            + "\n"
        )
        process.stdin.flush()
        response = json.loads(process.stdout.readline())
        self.assertEqual(response["id"], request_id)
        self.assertNotIn("error", response)
        return response["result"]


if __name__ == "__main__":
    unittest.main()
