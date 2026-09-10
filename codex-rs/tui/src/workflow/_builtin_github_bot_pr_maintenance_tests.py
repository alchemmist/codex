import importlib.util
import json
import pathlib
import unittest

PATH = pathlib.Path(__file__).with_name("builtin_github_bot_pr_maintenance.py")
SPEC = importlib.util.spec_from_file_location("workflow", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class Context:
    def __init__(self, report):
        self.params = {
            "owner": "owner",
            "action": "merge",
            "merge_method": "merge",
            "parallelism": 1,
            "max_repositories": 10,
        }
        self.state = {
            "repositories": [{"owner": "owner", "name": "repo"}],
            "next_index": 0,
            "results": [],
        }
        self.report = report
        self.requests = []
        self.retry_report = None
        self.retry_error = None

    def progress(self, *args, **kwargs):
        pass

    def checkpoint(self, state):
        self.state = state

    def agent(self, prompt, **kwargs):
        self.requests.append({"prompt": prompt, **kwargs})
        if self.retry_error:
            raise RuntimeError(self.retry_error)
        return {
            "success": True,
            "message": json.dumps(self.retry_report or self.report),
        }

    def agent_batch(self, requests, **kwargs):
        self.requests.extend(requests)
        return [{"success": True, "message": json.dumps(self.report)}]


class Tests(unittest.TestCase):
    def report(self, **kwargs):
        return dict(
            repository="owner/repo",
            candidates=1,
            merged=[],
            fixed=[],
            skipped=[],
            failed=[],
            summary="report",
            **kwargs,
        )

    def test_failed_run_is_not_completed(self):
        report = self.report()
        report["failed"] = [
            {"number": 453, "reason": "Workflow graph validation failed"}
        ]
        ctx = Context(report)
        with self.assertRaisesRegex(RuntimeError, "Workflow graph validation failed"):
            MODULE.run(ctx)
        self.assertEqual(ctx.state["results"], [report])

    def test_missing_candidate_is_invalid(self):
        with self.assertRaisesRegex(RuntimeError, "candidate"):
            MODULE.run(Context(self.report()))

    def test_failure_requires_reason(self):
        report = self.report()
        report["failed"] = [453]
        with self.assertRaisesRegex(RuntimeError, "reason"):
            MODULE.run(Context(report))

    def test_duplicate_candidate_is_invalid(self):
        report = self.report()
        report["candidates"] = 2
        report["merged"] = [453, 453]
        with self.assertRaisesRegex(RuntimeError, "candidate"):
            MODULE.run(Context(report))

    def test_failed_checkpoint_stays_failed_on_resume(self):
        report = self.report()
        report["failed"] = [{"number": 453, "reason": "MCP approval denied"}]
        ctx = Context(report)
        with self.assertRaisesRegex(RuntimeError, "MCP approval denied"):
            MODULE.run(ctx)
        with self.assertRaisesRegex(RuntimeError, "MCP approval denied"):
            MODULE.run(ctx)

    def test_review_only_skip_completes(self):
        report = self.report()
        report["skipped"] = [{"number": 453, "reason": "review-only mode"}]
        ctx = Context(report)
        ctx.params["action"] = "review"
        result = MODULE.run(ctx)
        self.assertEqual((result["merged"], result["skipped"]), (0, 1))

    def test_approval_review_is_scoped_to_mutating_agents(self):
        for action, archived, expected in [
            ("merge", False, "auto-review"),
            ("review", False, "inherit"),
            ("merge", True, "inherit"),
        ]:
            with self.subTest(action=action, archived=archived):
                report = self.report()
                report["skipped"] = [{"number": 453, "reason": "blocked"}]
                ctx = Context(report)
                ctx.params["action"] = action
                ctx.state["repositories"][0]["archived"] = archived
                MODULE.run(ctx)
                self.assertEqual(ctx.requests[0]["approval_mode"], expected)

    def test_retries_only_failed_candidates_and_preserves_successes(self):
        report = self.report()
        report.update(
            candidates=2,
            merged=[1],
            fixed=[1],
            failed=[{"number": 2, "reason": "mergeability unknown"}],
        )
        ctx = Context(report)
        ctx.retry_report = self.report()
        ctx.retry_report["merged"] = [2]
        result = MODULE.run(ctx)
        self.assertEqual(
            (result["merged"], result["fixed"], result["failed"]), (2, 1, 0)
        )
        self.assertEqual(len(ctx.requests), 2)
        self.assertIn("Retry only these PR numbers: [2]", ctx.requests[1]["prompt"])

    def test_retry_cannot_report_an_unrequested_pr(self):
        report = self.report()
        report["failed"] = [{"number": 2, "reason": "mergeability unknown"}]
        ctx = Context(report)
        ctx.retry_report = self.report()
        ctx.retry_report["merged"] = [99]
        with self.assertRaisesRegex(RuntimeError, "retry"):
            MODULE.run(ctx)
        self.assertEqual(ctx.state["results"][0]["merged"], [])

    def test_retry_is_bounded(self):
        report = self.report()
        report["failed"] = [{"number": 2, "reason": "logs unavailable"}]
        ctx = Context(report)
        with self.assertRaises(RuntimeError):
            MODULE.run(ctx)
        self.assertEqual(len(ctx.requests), 2)

    def test_retry_transport_error_preserves_completed_work(self):
        report = self.report()
        report.update(
            candidates=2, merged=[1], failed=[{"number": 2, "reason": "unknown"}]
        )
        ctx = Context(report)
        ctx.retry_error = "agent timed out"
        with self.assertRaisesRegex(RuntimeError, "retry failed: agent timed out"):
            MODULE.run(ctx)
        self.assertEqual(ctx.state["results"][0]["merged"], [1])

    def test_review_retry_does_not_enable_mutation_approvals(self):
        report = self.report()
        report["failed"] = [{"number": 2, "reason": "unknown"}]
        ctx = Context(report)
        ctx.params["action"] = "review"
        with self.assertRaises(RuntimeError):
            MODULE.run(ctx)
        self.assertEqual(ctx.requests[1]["approval_mode"], "inherit")

    def test_successful_merge(self):
        report = self.report()
        report["merged"] = [453]
        self.assertEqual(MODULE.run(Context(report))["merged"], 1)


if __name__ == "__main__":
    unittest.main()
