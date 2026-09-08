import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).parents[1]


class ExtensionTest(unittest.TestCase):
    def test_python_function_branches_after_host_action_result(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = pathlib.Path(directory)
            workflows = workspace / ".antex" / "workflows"
            workflows.mkdir(parents=True)
            (workflows / "check.py").write_text(
                "WORKFLOW={'id':'check','title':'Check'}\n"
                "def run(ctx):\n"
                " result=ctx.shell(['check',ctx.params['scope']])\n"
                " ctx.checkpoint({'exit':result['exitCode']})\n"
                " if result['exitCode']:\n"
                "  return ctx.agent('fix '+ctx.params['scope'])\n"
                " return {'ok':True}\n"
            )
            process = subprocess.Popen(
                [sys.executable, str(ROOT / "antex_ext_workflows.py")],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
                env={**os.environ, "ANTEX_TRUST_PROJECT_WORKFLOWS": "1"},
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
                        "cwd": str(workspace),
                        "capabilities": [
                            "agent",
                            "contextRead",
                            "persist",
                            "sessionRead",
                            "shell",
                            "ui",
                            "workspaceRead",
                            "workspaceWrite",
                        ],
                        "state": None,
                    },
                )
                first = self.request(
                    process,
                    "2",
                    "command/run",
                    {"name": "workflow", "arguments": 'check {"scope":"src"}'},
                )
                self.assertEqual(first["actions"][0]["command"], "check src")
                second = self.request(
                    process,
                    "3",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {
                            "id": "shell",
                            "succeeded": True,
                            "data": {"exitCode": 1, "text": "failed"},
                        },
                    },
                )
                self.assertEqual(second["records"][0]["state"], {"exit": 1})
                self.assertEqual(second["records"][0]["workflow"], "check")
                self.assertEqual(second["records"][0]["phase"], "running")
                self.assertEqual(second["actions"][0]["prompt"], "fix src")
                final = self.request(
                    process,
                    "4",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {
                            "id": "agent",
                            "succeeded": True,
                            "data": {"text": "fixed"},
                        },
                    },
                )
                self.assertEqual(json.loads(final["text"]), {"text": "fixed"})
            finally:
                process.kill()
                process.wait()
                process.stdin.close()
                process.stdout.close()

    def test_agent_batch_waits_for_every_result_in_original_order(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = pathlib.Path(directory)
            workflows = workspace / ".antex" / "workflows"
            workflows.mkdir(parents=True)
            (workflows / "batch.py").write_text(
                "WORKFLOW={'id':'batch','title':'Batch'}\n"
                "def run(ctx):\n"
                " return ctx.agent_batch(['first','second'],parallelism=2)\n"
            )
            process = subprocess.Popen(
                [sys.executable, str(ROOT / "antex_ext_workflows.py")],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
                env={**os.environ, "ANTEX_TRUST_PROJECT_WORKFLOWS": "1"},
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
                        "cwd": str(workspace),
                        "capabilities": [
                            "agent",
                            "contextRead",
                            "persist",
                            "sessionRead",
                            "shell",
                            "ui",
                            "workspaceRead",
                            "workspaceWrite",
                        ],
                        "state": None,
                    },
                )
                batch = self.request(
                    process,
                    "2",
                    "command/run",
                    {"name": "workflow", "arguments": "batch"},
                )
                self.assertEqual(
                    [action["prompt"] for action in batch["actions"]],
                    ["first", "second"],
                )
                pending = self.request(
                    process,
                    "3",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {"id": "agent-0", "succeeded": True, "data": {"text": "one"}},
                    },
                )
                self.assertIsNone(pending)
                final = self.request(
                    process,
                    "4",
                    "event/notify",
                    {
                        "name": "actionResult",
                        "data": {"id": "agent-1", "succeeded": True, "data": {"text": "two"}},
                    },
                )
                self.assertEqual(json.loads(final["text"]), [{"text": "one"}, {"text": "two"}])
            finally:
                process.kill()
                process.wait()
                process.stdin.close()
                process.stdout.close()

    def request(self, process, request_id, method, params):
        process.stdin.write(
            json.dumps(
                {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params},
                separators=(",", ":"),
            )
            + "\n"
        )
        process.stdin.flush()
        response = json.loads(process.stdout.readline())
        self.assertEqual(response["id"], request_id)
        self.assertNotIn("error", response)
        result = response["result"]
        if isinstance(result, dict) and "records" in result and not result["actions"] and (not result["records"] or result["records"][-1]["phase"] == "running"):
            return self.request(process, request_id, "command/run", {"name": "workflow", "arguments": "poll"})
        return result


if __name__ == "__main__":
    unittest.main()
