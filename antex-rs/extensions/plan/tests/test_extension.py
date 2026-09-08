import json
import pathlib
import subprocess
import sys
import unittest


ROOT = pathlib.Path(__file__).parents[1]


class ExtensionTest(unittest.TestCase):
    def test_todo_panel_updates_persists_and_restores(self):
        process = subprocess.Popen(
            [sys.executable, str(ROOT / "antex_ext_plan.py")],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        try:
            self.request(process, "1", "initialize", {"protocolVersion":1,"antexVersion":"0.0.0","sessionId":"test","cwd":"/tmp","capabilities":["persist","ui"],"state":{"items":[{"text":"old","done":True}]}})
            shown = self.request(process, "2", "command/run", {"name":"todo","arguments":""})
            self.assertEqual(shown["panel"]["text"], "- [x] old")
            updated = self.request(process, "3", "command/run", {"name":"todo","arguments":"new"})
            self.assertEqual(updated["records"][0]["items"][-1], {"text":"new","done":False})
            cleared = self.request(process, "4", "command/run", {"name":"todo-clear","arguments":""})
            self.assertEqual(cleared["panel"]["text"], "No TODO items.")
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
