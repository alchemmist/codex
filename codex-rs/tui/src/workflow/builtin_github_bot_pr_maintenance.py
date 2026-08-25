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
3. If CI is pending, wait and re-check within this agent run. If CI fails, diagnose the real cause.
   Fix only a bounded problem caused by the PR; never skip, delete, or weaken a test or quality gate.
4. If the PR conflicts, resolve it semantically against the current target branch. Prefer GitHub
   operations. If a local checkout is necessary, use a unique temporary directory and remove it.
   Update the PR with an ordinary commit only; if that is impossible without force push or without
   writing to the protected/default branch, leave the PR unmerged and report why.
5. Merge only after required reviews and every required CI check are successful, mergeability is
   confirmed, and the final diff is still limited to the bot PR's legitimate purpose.

{mutation_policy}

Return JSON only, with no markdown, in this exact shape:
{{
  "repository":"{owner}/{name}",
  "candidates":0,
  "merged":[],
  "fixed":[],
  "skipped":[{{"number":1,"reason":"precise reason"}}],
  "failed":[],
  "summary":"short factual summary"
}}
Include every candidate PR number in exactly one of merged, skipped, or failed. `merged` and `fixed`
contain PR numbers. In review-only mode, put otherwise mergeable PRs in skipped with reason
`review-only mode`.
"""


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
            }
            for repository in wave
        ]
        agent_results = ctx.agent_batch(requests, parallelism=parallelism)
        for repository, agent_result in zip(wave, agent_results):
            if agent_result.get("success"):
                try:
                    result = parse_json(agent_result.get("message", ""), dict)
                except (RuntimeError, json.JSONDecodeError) as error:
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
