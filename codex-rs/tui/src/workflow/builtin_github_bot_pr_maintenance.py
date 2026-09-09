import json

WORKFLOW = {
    "id": "github-bot-pr-maintenance",
    "title": "GitHub bot PR maintenance",
    "description": "Review or safely merge bot pull requests across owned GitHub repositories.",
    "version": 1,
    "fields": [
        {
            "id": "owner",
            "label": "GitHub owner",
            "description": "Only repositories owned by this account are considered.",
            "type": "text",
            "required": True,
            "default": "alchemmist",
            "placeholder": "github-login",
        },
        {
            "id": "action",
            "label": "Action",
            "description": "Review is read-only; Merge may update bot branches and merge verified PRs.",
            "type": "select",
            "options": [
                {
                    "value": "review",
                    "label": "Review only",
                    "description": "Inspect every candidate and produce a report without mutations.",
                },
                {
                    "value": "merge",
                    "label": "Merge",
                    "description": "Fix safe, bounded problems and merge only fully verified PRs.",
                },
            ],
            "default": "merge",
        },
        {
            "id": "merge_method",
            "label": "Merge method",
            "description": "Requested method when repository policy permits it.",
            "type": "select",
            "options": [
                {
                    "value": "squash",
                    "label": "Squash",
                    "description": "Create one commit per merged bot PR.",
                },
                {
                    "value": "merge",
                    "label": "Merge commit",
                    "description": "Preserve the pull request commit graph.",
                },
                {
                    "value": "rebase",
                    "label": "Rebase",
                    "description": "Replay pull request commits onto the target branch.",
                },
            ],
            "default": "squash",
        },
        {
            "id": "parallelism",
            "label": "Parallel repos",
            "description": "Each agent owns one repository, so repositories can run independently.",
            "type": "integer",
            "min": 1,
            "max": 15,
            "default": 5,
        },
        {
            "id": "model",
            "label": "Model override",
            "description": "Leave empty to use the configured Codex model.",
            "type": "text",
            "default": "",
            "placeholder": "gpt-5.6-sol",
        },
        {
            "id": "max_repositories",
            "label": "Repository limit",
            "description": "Hard cap protecting against an unexpectedly broad inventory.",
            "type": "integer",
            "min": 1,
            "max": 500,
            "default": 200,
        },
    ],
    "guardrails": {
        "max_agent_calls": 501,
        "max_shell_calls": 10,
        "max_parallel_agents": 15,
        "timeout_seconds": 86400,
    },
}


def parse_json(message, expected_type):
    text = message.strip()
    if text.startswith("```"):
        lines = text.splitlines()
        text = "\n".join(lines[1:-1]).strip()
        if text.startswith("json"):
            text = text[4:].lstrip()
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        opening = "[" if expected_type is list else "{"
        closing = "]" if expected_type is list else "}"
        start = text.find(opening)
        end = text.rfind(closing)
        if start < 0 or end < start:
            raise RuntimeError("agent did not return the required JSON result")
        value = json.loads(text[start : end + 1])
    if not isinstance(value, expected_type):
        raise TypeError("agent returned JSON with an unexpected top-level type")
    return value


def inventory_prompt(owner, limit):
    return f"""Use the GitHub MCP tools, not gh CLI, to inventory repositories owned directly by `{owner}`.

Return every repository owned by this account, including forks and archived repositories, up to the
hard safety limit of {limit} repositories.

Do not mutate GitHub. Verify ownership from repository metadata; do not include repositories merely
collaborated on. Return JSON only, with no markdown, in this exact shape:
[
  {{"name":"repo","owner":"{owner}","default_branch":"main","archived":false,"fork":false}}
]
Paginate until all pages have been read; a first page is not a complete inventory.
Sort by repository name. If inventory is incomplete or the result would exceed the limit, fail
clearly instead of silently truncating it.
"""


def repository_prompt(repository, action, merge_method):
    owner = repository["owner"]
    name = repository["name"]
    archived = repository.get("archived", False)
    mutation_policy = (
        "This is read-only review mode. Do not update branches, labels, comments, or pull requests."
        if action == "review" or archived
        else f"""This run explicitly authorizes merging qualifying PRs in this repository using
`{merge_method}` when repository policy permits. You may push a normal conflict-resolution or CI-fix
commit only to the existing PR branch when GitHub confirms maintainer modification is allowed.
Never force push, never push directly to the default branch, and never bypass branch protection."""
    )
    return f"""Maintain bot pull requests in GitHub repository `{owner}/{name}`.

Use GitHub MCP tools for GitHub reads and mutations; do not use gh CLI. Inspect every open PR and
consider it only when reliable GitHub metadata identifies its author as a bot. Accept `Bot` account
type, logins ending in `[bot]`, and well-known bot identities such as Dependabot or Renovate. Skip
ambiguous automation accounts, drafts, human-authored PRs, and PRs whose changes exceed their stated
automation purpose.

For every candidate:
1. Read its metadata, changed files, commits, reviews, mergeability, target branch, and all CI checks.
2. Check for suspicious dependency-source changes, unrelated generated files, weakened tests,
   disabled security checks, ignored failures, or other scope expansion. Never merge such a PR.
3. Read both get_status and get_check_runs for the current head SHA, including every page.
   A combined status of pending with total_count=0 means no legacy commit statuses exist;
   it is NOT a pending CI job. Evaluate actual status contexts and check runs separately.
   For example, pending with zero contexts and four successful checks does not block on CI.
   If both lists are empty, inspect required branch rules; do not invent a pending check or
   assume missing required checks succeeded. Distinguish optional checks from required checks.
   Wait only for actual pending statuses, queued/in-progress checks, or missing required checks.
   Re-check within this agent run. Use the GitHub Actions MCP tools to read failed job logs
   and check annotations. If these tools are missing, report that the actions toolset is required.
   If CI fails, read failure logs or annotations and diagnose
   the real cause. If tools cannot expose the logs, report that exact limitation.
   Fix only a bounded problem caused by the PR; never skip, delete, or weaken a test or quality gate.
   Compare with the target branch before calling a failure pre-existing. Report the root cause,
   attempted repair, and precise blocker if a candidate cannot be fixed.
4. If the PR conflicts, resolve it semantically against the current target branch. Prefer GitHub
   operations. If a local checkout is necessary, use a unique temporary directory and remove it.
   Update the PR with an ordinary commit only; if that is impossible without force push or without
   writing to the protected/default branch, leave the PR unmerged and report why.
5. Merge only after required reviews and every required CI check are successful, mergeability is
   confirmed, and the final diff is still limited to the bot PR's legitimate purpose.
   A requested reviewer alone does not establish a required approval. Inspect branch rules;
   never bypass a confirmed protection or merge while GitHub reports blocked mergeability.
   Refresh the head and checks immediately before merge and pass the verified head SHA to the
   merge operation. Confirm the merged state from GitHub before reporting success.
6. In merge mode, attempt an authorized qualifying MCP operation instead of inferring an
   approval denial from the environment description. If the tool actually rejects it, report
   the exact operation and error in failed. Do not change approval policy or bypass the denial.

{mutation_policy}

Return JSON only, with no markdown, in this exact shape:
{{
  "repository":"{owner}/{name}",
  "candidates":2,
  "merged":[],
  "fixed":[],
  "skipped":[{{"number":1,"reason":"precise reason"}}],
  "failed":[{{"number":2,"reason":"precise error or blocker"}}],
  "summary":"short factual summary"
}}
Include every candidate PR number in exactly one of merged, skipped, or failed. `merged` and `fixed`
contain PR numbers. Both skipped and failed contain objects with a PR number and nonempty reason;
use empty arrays when there are no entries. In review-only mode, put otherwise mergeable PRs in skipped with reason
`review-only mode`.
"""


def validate_report(result, repository):
    if result.get("repository") != f"{repository['owner']}/{repository['name']}":
        raise RuntimeError("agent report has the wrong repository")
    candidates = result.get("candidates")
    if type(candidates) is not int or candidates < 0:
        raise RuntimeError("agent report has an invalid candidate count")
    numbers = []
    for category in ("merged", "fixed", "skipped", "failed"):
        entries = result.get(category)
        if not isinstance(entries, list):
            raise TypeError(f"agent report is missing {category} entries")
        for entry in entries:
            number = entry
            if category in ("skipped", "failed"):
                if (
                    not isinstance(entry, dict)
                    or not isinstance(entry.get("reason"), str)
                    or not entry["reason"].strip()
                ):
                    raise RuntimeError(f"{category} entry requires a precise reason")
                number = entry.get("number")
            if type(number) is not int or number <= 0:
                raise RuntimeError(f"{category} entry requires a PR number")
            if category != "fixed":
                numbers.append(number)
    if len(numbers) != candidates or len(set(numbers)) != candidates:
        raise RuntimeError("agent report must account for every candidate exactly once")
    if not set(result["fixed"]).issubset(numbers):
        raise RuntimeError("fixed PR is not a reported candidate")


def normalize_repositories(repositories, owner, limit):
    normalized = []
    seen = set()
    for repository in repositories:
        if not isinstance(repository, dict):
            continue
        name = str(repository.get("name", "")).strip()
        repo_owner = str(repository.get("owner", "")).strip()
        key = name.casefold()
        if not name or repo_owner.casefold() != owner.casefold() or key in seen:
            continue
        seen.add(key)
        normalized.append(
            {
                "name": name,
                "owner": repo_owner,
                "default_branch": str(repository.get("default_branch", "")),
                "archived": bool(repository.get("archived", False)),
                "fork": bool(repository.get("fork", False)),
            }
        )
    normalized.sort(key=lambda repository: repository["name"].casefold())
    if len(normalized) > limit:
        raise RuntimeError(f"repository inventory exceeds configured limit of {limit}")
    return normalized


def run(ctx):
    owner = ctx.params["owner"].strip()
    action = ctx.params["action"]
    merge_method = ctx.params["merge_method"]
    parallelism = ctx.params["parallelism"]
    model = ctx.params.get("model") or None
    limit = ctx.params["max_repositories"]
    state = ctx.state or {}

    repositories = state.get("repositories")
    if repositories is None:
        ctx.progress("Inventorying owned GitHub repositories", current=0, total=1)
        inventory = ctx.agent(
            inventory_prompt(owner, limit),
            model=model,
            timeout_seconds=1800,
        )
        if not inventory.get("success"):
            raise RuntimeError(
                inventory.get("error") or "repository inventory agent failed"
            )
        repositories = normalize_repositories(
            parse_json(inventory.get("message", ""), list),
            owner,
            limit,
        )
        state = {"repositories": repositories, "next_index": 0, "results": []}
        ctx.checkpoint(state)

    next_index = int(state.get("next_index", 0))
    results = list(state.get("results", []))
    total = len(repositories)
    if next_index > total:
        raise RuntimeError("workflow checkpoint points past the repository inventory")

    for offset in range(next_index, total, parallelism):
        wave = repositories[offset : offset + parallelism]
        names = ", ".join(repository["name"] for repository in wave)
        ctx.progress(
            f"Repositories {offset + 1}-{offset + len(wave)}: {names}",
            current=offset,
            total=total,
        )
        requests = [
            {
                "prompt": repository_prompt(repository, action, merge_method),
                "model": model,
                "timeout_seconds": 7200,
                "approval_mode": (
                    "auto-review"
                    if action == "merge" and not repository.get("archived", False)
                    else "inherit"
                ),
            }
            for repository in wave
        ]
        agent_results = ctx.agent_batch(requests, parallelism=parallelism)
        for repository, agent_result in zip(wave, agent_results):
            if agent_result.get("success"):
                try:
                    result = parse_json(agent_result.get("message", ""), dict)
                    validate_report(result, repository)
                except (RuntimeError, TypeError, json.JSONDecodeError) as error:
                    result = {
                        "repository": f"{repository['owner']}/{repository['name']}",
                        "candidates": 0,
                        "merged": [],
                        "fixed": [],
                        "skipped": [],
                        "failed": [{"reason": str(error)}],
                        "summary": "Agent returned an invalid report.",
                    }
            else:
                result = {
                    "repository": f"{repository['owner']}/{repository['name']}",
                    "candidates": 0,
                    "merged": [],
                    "fixed": [],
                    "skipped": [],
                    "failed": [
                        {
                            "reason": (agent_result.get("error") or "agent failed")[
                                -2000:
                            ]
                        }
                    ],
                    "summary": "Repository agent failed.",
                }
            results.append(result)
        state = {
            "repositories": repositories,
            "next_index": offset + len(wave),
            "results": results,
        }
        ctx.checkpoint(state)

    merged = sum(len(result.get("merged", [])) for result in results)
    fixed = sum(len(result.get("fixed", [])) for result in results)
    skipped = sum(len(result.get("skipped", [])) for result in results)
    failed = sum(len(result.get("failed", [])) for result in results)
    if failed:
        failures = [
            f"{result['repository']} #{entry.get('number', '?') if isinstance(entry, dict) else entry}: "
            f"{entry.get('reason', 'missing failure reason') if isinstance(entry, dict) else 'missing failure reason'}"
            for result in results
            for entry in result.get("failed", [])
        ]
        raise RuntimeError(
            f"Bot PR maintenance incomplete: merged={merged}, fixed={fixed}, "
            f"skipped={skipped}, failed={failed}. Reports saved in workflow state. "
            + "; ".join(failures)[:6000]
        )
    ctx.progress("GitHub bot PR maintenance completed", current=total, total=total)
    return {
        "action": action,
        "owner": owner,
        "repositories": total,
        "merged": merged,
        "fixed": fixed,
        "skipped": skipped,
        "failed": failed,
        "results": results,
    }
