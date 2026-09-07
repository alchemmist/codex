import json
import pathlib
import subprocess
import sys
import unittest


ROOT = pathlib.Path(__file__).parents[1]


class ExtensionTest(unittest.TestCase):
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
