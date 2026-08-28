import importlib.util
import pathlib
import sys


def load_workflow(path):
    spec = importlib.util.spec_from_file_location("pr_babysitter", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.time.sleep = lambda _seconds: None
    return module


class FakeContext:
    def __init__(self, module, monitor_results, worker_results=None):
        self.module = module
        self.monitor_results = iter(monitor_results)
        self.worker_results = iter(worker_results or [])
        self.params = {
            "pull_request": "https://github.com/acme/repo/pull/7",
            "poll_interval_seconds": 30,
            "monitor_model": "gpt-5.6-luna",
            "monitor_reasoning": "low",
            "worker_model": "gpt-5.6-sol",
            "worker_reasoning": "high",
            "stable_green_polls": 1,
            "max_worker_failures": 3,
        }
        self.state = {}
        self.checkpoints = []
        self.agent_calls = []

    def agent(self, prompt, **kwargs):
        self.agent_calls.append((prompt, kwargs))
        if prompt.startswith("Inspect pull request"):
            value = next(self.monitor_results)
        else:
            assert kwargs["developer_instructions"] == self.module.WORKER_POLICY
            assert kwargs["forbid_quality_graph_ignore"] is True
            value = next(self.worker_results)
        return {"success": True, "message": self.module.json.dumps(value), "error": ""}

    def checkpoint(self, state):
        self.state = dict(state)
        self.checkpoints.append(dict(state))

    def progress(self, _message, current=None, total=None):
        return (current, total)


def snapshot(ci_status="green", failed_checks=None, feedback=None):
    failed_checks = failed_checks or []
    feedback = feedback or []
    failed = len(failed_checks)
    return {
        "repository": "acme/repo",
        "number": 7,
        "url": "https://github.com/acme/repo/pull/7",
        "state": "open",
        "head_sha": "a" * 40,
        "mergeable": True,
        "review_decision": "approved",
        "ci_status": ci_status,
        "checks": {"passed": 3 - failed, "pending": 0, "failed": failed, "total": 3},
        "failed_checks": failed_checks,
        "unresolved_feedback": feedback,
    }


def main():
    module = load_workflow(pathlib.Path(sys.argv[1]))

    ready_context = FakeContext(module, [snapshot()])
    ready = module.run(ready_context)
    assert ready["status"] == "ready"
    assert len(ready_context.agent_calls) == 1
    assert ready_context.agent_calls[0][1]["model"] == "gpt-5.6-luna"
    assert ready_context.agent_calls[0][1]["reasoning_effort"] == "low"

    failure = {"id": "check-1", "name": "tests", "summary": "failed", "url": "url"}
    worker_context = FakeContext(
        module,
        [snapshot("failing", [failure]), snapshot()],
        [
            {
                "status": "changed",
                "summary": "fixed tests",
                "commit": "b" * 40,
                "handled_ids": ["check-1"],
                "needs_user_reason": "",
            }
        ],
    )
    repaired = module.run(worker_context)
    assert repaired["status"] == "ready"
    assert repaired["worker_calls"] == 1
    assert len(worker_context.agent_calls) == 3

    assert module.forbidden_quality_graph_text({"message": "/qg finding ignore"})
    assert not module.forbidden_quality_graph_text({"message": "fixed root cause"})


if __name__ == "__main__":
    main()
