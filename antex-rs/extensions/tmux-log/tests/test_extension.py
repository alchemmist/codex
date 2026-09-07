import json
import pathlib
import subprocess
import sys
import unittest


ROOT = pathlib.Path(__file__).parents[1]


class ExtensionTest(unittest.TestCase):
    def setUp(self):
        self.process = subprocess.Popen(
            [sys.executable, str(ROOT / "antex_ext_tmux_log.py")],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )

    def tearDown(self):
        self.process.kill()
        self.process.wait()
        self.process.stdin.close()
        self.process.stdout.close()

    def request(self, request_id, method, params):
        self.process.stdin.write(
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
        self.process.stdin.flush()
        response = json.loads(self.process.stdout.readline())
        self.assertEqual(response["id"], request_id)
        self.assertNotIn("error", response)
        return response["result"]

    def test_toggle_and_shell_lifecycle_emit_terminal_log_actions(self):
        manifest = self.request(
            "1",
            "initialize",
            {
                "protocolVersion": 1,
                "antexVersion": "0.0.0",
                "sessionId": "test",
                "cwd": "/tmp",
                "capabilities": ["persist", "ui"],
                "state": None,
            },
        )
        self.assertEqual(manifest["commands"][0]["name"], "tmux-command-log")
        enabled = self.request(
            "2",
            "command/run",
            {"name": "tmux-command-log", "arguments": ""},
        )
        self.assertEqual(enabled["records"], [{"enabled": True}])
        started = self.request(
            "3",
            "event/notify",
            {
                "name": "toolStarted",
                "data": {
                    "callId": "call-1",
                    "name": "shell",
                    "arguments": {"command": "pwd"},
                },
            },
        )
        self.assertEqual(started["actions"][0]["text"], "$ pwd")
        completed = self.request(
            "4",
            "event/notify",
            {
                "name": "toolCompleted",
                "data": {"callId": "call-1", "outcome": "success", "text": "/tmp"},
            },
        )
        self.assertEqual(completed["actions"][0]["text"], "/tmp\n[success]")

    def test_restored_enabled_state_logs_without_another_toggle(self):
        self.request(
            "1",
            "initialize",
            {
                "protocolVersion": 1,
                "antexVersion": "0.0.0",
                "sessionId": "test",
                "cwd": "/tmp",
                "capabilities": ["persist", "ui"],
                "state": {"enabled": True},
            },
        )
        started = self.request(
            "2",
            "event/notify",
            {
                "name": "toolStarted",
                "data": {
                    "callId": "call-1",
                    "name": "shell",
                    "arguments": {"command": "pwd"},
                },
            },
        )
        self.assertEqual(started["actions"][0]["text"], "$ pwd")


if __name__ == "__main__":
    unittest.main()
