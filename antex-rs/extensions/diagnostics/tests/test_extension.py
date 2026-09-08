import json
import pathlib
import subprocess
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).parents[1]


class ExtensionTest(unittest.TestCase):
    def test_context_panel_and_non_destructive_transcript_export(self):
        with tempfile.TemporaryDirectory() as directory:
            process = subprocess.Popen(
                [sys.executable, str(ROOT / "antex_ext_diagnostics.py")],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
            )
            try:
                self.request(
                    process,
                    "1",
                    "initialize",
                    {
                        "protocolVersion": 1,
                        "antexVersion": "0.0.0",
                        "sessionId": "test",
                        "cwd": directory,
                        "capabilities": ["contextRead", "sessionRead", "ui", "workspaceWrite"],
                        "state": None,
                    },
                )
                requested = self.request(
                    process,
                    "2",
                    "command/run",
                    {"name": "context", "arguments": ""},
                )
                self.assertEqual(requested["actions"][0]["target"], "context")
                panel = self.request(
                    process,
                    "3",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {"id": "inspect", "succeeded": True, "data": {"text": "visible"}},
                    },
                )
                self.assertEqual(panel["panel"], {"title": "Model context", "text": "visible"})
                self.request(
                    process,
                    "4",
                    "command/run",
                    {"name": "dump", "arguments": "report.md"},
                )
                exported = self.request(
                    process,
                    "5",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {"id": "inspect", "succeeded": True, "data": {"text": "transcript"}},
                    },
                )
                self.assertEqual(exported["text"], "Exported transcript to report.md")
                self.assertEqual(pathlib.Path(directory, "report.md").read_text(), "transcript")
                self.request(
                    process,
                    "6",
                    "command/run",
                    {"name": "dump", "arguments": "report.md"},
                )
                error = self.raw_request(
                    process,
                    "7",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {"id": "inspect", "succeeded": True, "data": {"text": "changed"}},
                    },
                )
                self.assertIn("error", error)
                self.assertEqual(pathlib.Path(directory, "report.md").read_text(), "transcript")
            finally:
                process.kill()
                process.wait()
                process.stdin.close()
                process.stdout.close()

    def raw_request(self, process, request_id, method, params):
        process.stdin.write(json.dumps({"jsonrpc":"2.0","id":request_id,"method":method,"params":params}) + "\n")
        process.stdin.flush()
        return json.loads(process.stdout.readline())

    def request(self, process, request_id, method, params):
        response = self.raw_request(process, request_id, method, params)
        self.assertNotIn("error", response)
        return response["result"]


if __name__ == "__main__":
    unittest.main()
