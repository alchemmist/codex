import hashlib
import importlib.util
import os
import pathlib
import tempfile
import unittest
import time
from unittest.mock import patch


PROGRAM = pathlib.Path(__file__).parents[1] / "antex_ext_workflows.py"
SPEC = importlib.util.spec_from_file_location("workflows", PROGRAM)
WORKFLOWS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WORKFLOWS)
SOURCE = "WORKFLOW={'id':'check'}\ndef run(ctx):\n return {'ok':True}\n"


def run_to_output(runner, arguments):
    result = runner.start(arguments)
    deadline = time.monotonic() + 2
    while not result["actions"] and (not result["records"] or result["records"][-1]["phase"] == "running"):
        if time.monotonic() > deadline:
            raise AssertionError("workflow did not complete")
        result = runner.start("poll")
    return result


class DiscoveryTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.personal = self.root / "personal"
        self.personal.mkdir()
        self.project = self.root / ".antex" / "workflows"
        self.project.mkdir(parents=True)
        environment = patch.dict(os.environ, {"ANTEX_WORKFLOWS_DIR": str(self.personal),
                                              "ANTEX_TRUST_PROJECT_WORKFLOWS": "0"})
        environment.start()
        self.addCleanup(environment.stop)

    def test_project_code_requires_trust_even_when_explicitly_named(self):
        (self.project / "check.py").write_text(SOURCE)
        runner = WORKFLOWS.Runner(self.root, None)
        self.assertEqual(WORKFLOWS.workflow_paths(self.root), {})
        with self.assertRaisesRegex(ValueError, "not found"):
            runner.start("check")
        with patch.dict(os.environ, {"ANTEX_TRUST_PROJECT_WORKFLOWS": "1"}):
            result = run_to_output(runner, "check")
        self.assertEqual(result["text"], '{"ok": true}')

    def test_personal_workflow_retains_exact_source_and_new_run_identity(self):
        (self.personal / "check.py").write_text(SOURCE)
        runner = WORKFLOWS.Runner(self.root, {"workflow": "check", "state": {"old": True}})
        result = run_to_output(runner, "check")
        record = result["records"][0]
        self.assertEqual(record["source"], SOURCE)
        self.assertEqual(record["sourceSha256"], hashlib.sha256(SOURCE.encode()).hexdigest())
        self.assertEqual(record["phase"], "completed")
        self.assertEqual(record["state"], {})
        runner.thread.join()
        second = run_to_output(runner, "check")
        self.assertNotEqual(second["records"][0]["runId"], record["runId"])

    def test_failed_import_is_persisted_with_its_source(self):
        source = "raise RuntimeError('broken workflow')\n"
        (self.personal / "check.py").write_text(source)
        result = run_to_output(WORKFLOWS.Runner(self.root, None), "check")
        self.assertEqual(result["text"], "Workflow failed: broken workflow")
        self.assertEqual(result["records"][0]["phase"], "failed")
        self.assertEqual(result["records"][0]["source"], source)

    def test_duplicates_and_symlinks_do_not_select_unintended_code(self):
        (self.personal / "check.py").write_text(SOURCE)
        (self.project / "check.py").write_text(SOURCE)
        with patch.dict(os.environ, {"ANTEX_TRUST_PROJECT_WORKFLOWS": "1"}):
            with self.assertRaisesRegex(ValueError, "duplicate"):
                WORKFLOWS.workflow_paths(self.root)
        (self.personal / "link.py").symlink_to(self.project / "check.py")
        self.assertEqual(list(WORKFLOWS.workflow_paths(self.root)), ["check"])

    def test_unsupported_agent_options_fail_before_scheduling(self):
        context = WORKFLOWS.Context({}, {}, None, None)
        with self.assertRaisesRegex(ValueError, "unsupported"):
            context.agent("task", cwd="elsewhere")
        with self.assertRaisesRegex(ValueError, "unsupported"):
            context.agent_batch(["task"], developer_instructions="instructions")

    def test_resume_uses_checkpoint_and_saved_source_after_the_file_changes(self):
        source = "WORKFLOW={'id':'check'}\ndef run(ctx):\n if ctx.state.get('done'): return True\n ctx.checkpoint({'done':True})\n raise RuntimeError('interrupted')\n"
        path = self.personal / "check.py"
        path.write_text(source)
        record = run_to_output(WORKFLOWS.Runner(self.root, None), "check")["records"][0]
        path.write_text("raise RuntimeError('edited source must not run')\n")
        restored = WORKFLOWS.Runner(self.root, record)
        result = run_to_output(restored, "resume")
        self.assertEqual(result["text"], "true")
        self.assertEqual(result["records"][0]["runId"], record["runId"])
        self.assertEqual(result["records"][0]["source"], source)
        self.assertEqual(result["records"][0]["state"], {"done": True})

    def test_running_checkpoint_reopens_as_interrupted_and_rejects_corrupt_source(self):
        (self.personal / "check.py").write_text(SOURCE)
        runner = WORKFLOWS.Runner(self.root, {"workflow": "check", "phase": "running",
                                            "source": SOURCE, "sourceSha256": "wrong"})
        self.assertEqual(runner.start("status")["text"], "interrupted")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            runner.start("resume")


if __name__ == "__main__":
    unittest.main()
