import hashlib
import importlib.util
import os
import pathlib
import tempfile
import unittest
from unittest.mock import patch


PROGRAM = pathlib.Path(__file__).parents[1] / "antex_ext_workflows.py"
SPEC = importlib.util.spec_from_file_location("workflows", PROGRAM)
WORKFLOWS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WORKFLOWS)
SOURCE = "WORKFLOW={'id':'check'}\ndef run(ctx):\n return {'ok':True}\n"


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
            result = runner.start("check")
        self.assertEqual(result["text"], '{"ok": true}')

    def test_personal_workflow_retains_exact_source_and_new_run_identity(self):
        (self.personal / "check.py").write_text(SOURCE)
        runner = WORKFLOWS.Runner(self.root, {"workflow": "check", "state": {"old": True}})
        result = runner.start("check")
        record = result["records"][0]
        self.assertEqual(record["source"], SOURCE)
        self.assertEqual(record["sourceSha256"], hashlib.sha256(SOURCE.encode()).hexdigest())
        self.assertEqual(record["phase"], "completed")
        self.assertEqual(record["state"], {})
        runner.thread.join()
        second = runner.start("check")
        self.assertNotEqual(second["records"][0]["runId"], record["runId"])

    def test_failed_import_is_persisted_with_its_source(self):
        source = "raise RuntimeError('broken workflow')\n"
        (self.personal / "check.py").write_text(source)
        result = WORKFLOWS.Runner(self.root, None).start("check")
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


if __name__ == "__main__":
    unittest.main()
