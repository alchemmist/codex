import json
import pathlib
import subprocess
import sys
import unittest


ROOT = pathlib.Path(__file__).parents[1]


class ExtensionTest(unittest.TestCase):
    def test_agents_require_explicit_commands_and_persist_bounded_results(self):
        process = subprocess.Popen(
            [sys.executable, str(ROOT / "antex_ext_agents.py")],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        try:
            manifest = self.request(
                process,
                "1",
                "initialize",
                {
                    "protocolVersion": 1,
                    "antexVersion": "0.0.0",
                    "sessionId": "test",
                    "cwd": "/tmp",
                    "capabilities": ["agent", "persist", "ui"],
                    "state": None,
                },
            )
            self.assertEqual(
                [command["name"] for command in manifest["commands"]],
                ["subagents", "agents"],
            )
            batch = self.request(
                process,
                "2",
                "command/run",
                {"name": "agents", "arguments": '["one","two"]'},
            )
            self.assertEqual(
                [action["prompt"] for action in batch["actions"]],
                ["one", "two"],
            )
            self.assertIsNone(
                self.request(
                    process,
                    "3",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {"id": "agent-0", "succeeded": True, "data": {"text": "first"}},
                    },
                )
            )
            result = self.request(
                process,
                "4",
                "event/notify",
                {
                    "name": "actionResult",
                    "data": {"id": "agent-1", "succeeded": False, "data": {"text": "second"}},
                },
            )
            self.assertEqual(
                result["records"][0]["runs"],
                [
                    {"prompt": "one", "status": "completed"},
                    {"prompt": "two", "status": "failed"},
                ],
            )
            self.assertIn("first", result["panel"]["text"])
            self.assertIn("second", result["panel"]["text"])
        finally:
            process.kill()
            process.wait()
            process.stdin.close()
            process.stdout.close()

    def request(self, process, request_id, method, params):
        process.stdin.write(json.dumps({"jsonrpc":"2.0","id":request_id,"method":method,"params":params}) + "\n")
        process.stdin.flush()
        response = json.loads(process.stdout.readline())
        self.assertNotIn("error", response)
        return response["result"]


if __name__ == "__main__":
    unittest.main()
