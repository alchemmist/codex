# Antex handoff

Date: 2026-09-07. Branch: `antex`. Checkpoint: `fbcced88c7246b35774b3517117d6d7874a1a11c`.

## Product boundary

Antex is an independent terminal coding agent with one shipped executable,
`antex`. Login, provider calls, the agent loop, built-in tools, sessions,
compaction, and the TUI run in that process. There is no required app-server,
daemon, IPC service, desktop process, V8 host, or companion binary.

The base agent works without extensions. First-party extensions are embedded as
small resources and materialized only through `antex extensions install <name>`.
Their processes are optional, sandboxed, bounded, restartable, and keep state in
the append-only Antex session. Complex orchestration belongs in workflows rather
than the kernel or system prompt.

The old local fallback remains `/Users/antonmoss/.local/bin/codex`, version
`0.0.14`, with SHA-256
`be16a880b76ea5c6d4a38e61ff1f4fa86de15306078513d639cc1716df3528b2`.
Do not replace it before every Phase 10 cutover gate passes.

## Repository state

Local development install on 2026-09-08:
`/Users/antonmoss/.local/bin/antex`, version `0.0.0+d6ec69910f`.
It was cross-built on deimos, transferred with matching checksum and ad-hoc
signed on macOS. Interactive zsh resolves `antex` to this exact path; `.zshrc`
already included `~/.local/bin`, so no shell configuration edit was needed.
The maintainer's ANSI/Vim/status-line preferences and existing full-access
profile are now reflected in `~/.antex/config.toml`; the Codex fallback and its
config were not modified. The full-access preference was restored after the
initial workspace/no-network default caused unexpected restrictions.
The TUI restoration and remaining parity gate are documented in
`migration/tui-parity.md`. Do not equate this development install with Phase 10.

- Rust workspace: `antex-rs/`.
- Seven production crates: CLI, core, runtime, OpenAI provider, TUI, extension
  protocol, and extension host.
- One Cargo binary target: `antex`.
- Product version: `0.0.0`; private OpenAI compatibility revision: `0.153.4`.
- Origin is still `git@github.com:alchemmist/codex.git`.
- No `antex-v*` tag or GitHub Release exists.
- The GitHub repository has not been renamed.
- User-owned untracked `research/` must remain untouched and uncommitted.
- Stash `antex session migration wip before legacy deletion` is obsolete: its
  session and prompt-stash migration was incorporated. It can be dropped later
  after final audit, but is harmless.

The legacy tracked Codex workspace and product infrastructure were deleted.
The ignored local `codex-rs/target` was deliberately retained. Git history and
all historical upstream and `alchemmist-v*` tags remain intact.

## Current measured inventory

`scripts/antex-baseline.py --check` on deimos reports:

- 35,032 production Rust lines;
- `antex-core`: 1,576 lines;
- `antex-tui`: 25,177 lines;
- 446 production dependency nodes;
- zero forbidden dependencies;
- zero production modules above 800 lines;
- one shipped executable, `antex`;
- 1,601-byte built-in system prompt.

The gate rejects additional binary targets, more than ten production crates,
more than 100,000 production lines, `antex-core` above 12,000 lines, modules
above 800 lines, forbidden dependencies, and system prompts above 4 KiB.

## Implemented product path

- Provider-neutral `Agent` with deterministic assistant/tool iterations,
  steering, follow-up, interruption, bounded retries, and context hooks.
- OpenAI adapter with ChatGPT account storage, refresh, browser/device login,
  model discovery, Responses streaming, reasoning, usage, and private protocol
  compatibility revision.
- Runtime `read`, `write`, `edit`, and `shell` tools; read-only/workspace/full
  permissions; Bubblewrap/seccomp on Linux and Seatbelt on macOS.
- Append-only JSONL sessions with resume, parent-linked fork, torn-tail recovery,
  pending input recovery, compaction checkpoints, UI state, and extension state.
- Explicit non-destructive Codex migration for supported config, skills, stdio
  and HTTP MCP definitions, portable sessions, and plain prompt stashes.
- Direct inline TUI with retained mascot, Markdown, diffs, images, clipboard,
  themes, Vim/Russian aliases, prompt stash, queues, pickers, transcript, and
  narrow-terminal behavior.
- Bounded JSON-RPC extension protocol and host with discovery, project trust,
  capability sandbox, timeouts, cancellation, restart limits, failure isolation,
  Rust/Python conformance, command routing, lifecycle events, and action chains.
- Explicit extension actions: sandboxed shell, context/session inspection,
  ephemeral in-process agents, parallel batches up to eight, and terminal logs.
  Model tools and lifecycle observers cannot spawn agents.

## First-party extensions

All are optional. The first five are explicitly installed from the one Antex
binary; configured MCP instances are emitted by `antex migrate codex`:

- `tmux-log`: `/tmux-command-log`, bounded private tmux window, persisted toggle.
- `workflows`: project `.antex/workflows/<id>.py`, synchronous `ctx.shell`,
  `ctx.agent`, parallel `ctx.agent_batch`, `ctx.inspect`, checkpoint, progress,
  and logging over a 64-action continuation loop.
- `agents`: `/subagents`, `/agents`, up to eight explicit ephemeral agents and
  bounded persisted run summaries.
- `diagnostics`: `/context`, `/system-prompt`, and non-overwriting `/dump`.
- `plan`: persistent `/todo` panel with at most 64 items per session branch.
- `mcp`: stdio and Streamable HTTP transports. HTTP supports JSON or SSE POST
  responses, `Mcp-Session-Id`, `MCP-Protocol-Version: 2025-06-18`, loopback HTTP
  or HTTPS enforcement, response bounds, and a private bearer-token file.

## Latest validation evidence

- Full TUI suite before the extension-only slices: 782 passed, one existing skip.
- Runtime/CLI/extension action slice: 71 passed on deimos.
- Workflow/extension-host/CLI slice: 31 passed on deimos.
- Clean rebuild after removing the disposable remote target: 31 passed.
- HTTP MCP Python acceptance: two tests pass, covering stdio plus HTTP JSON/SSE,
  session/protocol headers, and bearer headers.
- Agents, workflows, diagnostics, plan, and tmux-log Python protocol acceptance
  tests pass.
- Current workspace `just clippy -- -D warnings` passes on deimos.
- Current HTTP MCP Codex-migration test passes on deimos.
- On 2026-09-08 the user approved the full workspace run: 905 passed, one
  existing skip, 4.914 seconds after 22.37 seconds of compilation on deimos.
  All 16 Python extension acceptance tests passed there as well. This approval
  remains valid for the remaining migration validation.

deimos filled its filesystem during linking. Only the disposable directory
`/home/antonmoss/antex-work/validation-host/antex-rs/target` was removed and
recreated, releasing about 30 GiB. The clean scoped rebuild then passed. Check
disk space before another full build.

## Remaining work, in recommended order

2026-09-08 continuation: workflow discovery now includes personal workflows and
requires project trust for project code. Duplicate IDs and symlink files fail
closed. Runs persist bounded source snapshots, hashes, parameters, identities,
timestamps and completion/failure metadata. Agent batches honor parallelism;
unsupported agent options fail explicitly. Seven Python acceptance tests and
19 CLI tests passed; local `just fmt` and remote scoped `just fix` passed.
The personal-workflow sandbox test verifies adjacent credentials stay hidden.
The following slice adds the workflow picker, status and explicit snapshot-based
resume. Host action events no longer replace extension checkpoint state after
reopening or branching a session. Nine Python tests and 68 runtime/CLI tests
passed, followed by local formatting and scoped remote Clippy fix with warnings
denied. The subsequent full-suite evidence is recorded above.

1. Package the PR babysitter as a workflow. Background workflow execution,
   pause at host-action boundaries, stop with process-group cancellation and
   checkpoint-based resume are implemented. Session writes are acknowledged
   before actions start. Full deimos validation passed 906 tests (one existing
   skip); the final persistence adjustment passed 69 runtime/CLI tests. Python
   workflow tests and final workspace Clippy fix passed. Control notices have
   reviewed snapshots, and a shell test verifies stopped descendants cannot
   write later and that the run can resume from its checkpoint.
2. Finish MCP OAuth according to the current official MCP authorization spec:
   - parse `WWW-Authenticate` and RFC 9728 protected-resource metadata;
   - discover RFC 8414 authorization-server metadata;
   - support dynamic client registration when advertised;
   - implement authorization code + PKCE + state + localhost callback;
   - include RFC 8707 `resource` in authorization and token requests;
   - store/refresh/rotate tokens privately and audience-bind them;
   - add fake-server tests for discovery, login, refresh, 401, scope failure,
     redirects, and token non-disclosure.
   The current bearer-token-file mode is not full OAuth completion.
3. Run the complete workspace suite on deimos with
   `ANTEX_BWRAP=/home/antonmoss/antex-tools/bin/bwrap`, then release Clippy,
   baseline, and extension conformance.
4. Finish live interruption and the remaining interactive acceptance scenarios.
   Device login, model discovery, plain turns, write/edit/read/shell, resume and
   fork succeeded on deimos. A separately authenticated macOS binary completed
   model discovery and a direct TUI turn. Its initial four-tool smoke was later
   found to contain a failed shell result despite the model's success response.
   See `migration/macos-shell-regression.md` and
   `migration/live-acceptance.md` and `migration/macos-cross-build.md`.
5. Resolve the OpenAI identification experiment: remove the compatibility
   revision if the endpoint accepts native Antex identification, otherwise keep
   the private contract pin and document its update procedure.
6. Complete supported-platform preflight:
   - complete Apple Silicon macOS acceptance using the successful deimos
     cross-build with the maintainer's SDK and local ad-hoc signing;
   - Linux x86_64 fresh clone;
   - stripped binary size below 40 MiB on both;
   - editable prompt below 150 ms;
   - idle RSS below 60 MiB;
   - warm full suite below five minutes;
   - narrow tmux and light/dark theme smoke;
   - explicit MCP/workflow/tmux/agents/Codex-migration smoke.
7. Test `make install-local`, `make install-mac`, and `make install-linux`
   against draft artifacts and verify they install only `antex`.
8. At final cutover only: rename GitHub repository to `alchemmist/antex`, update
   `origin`, verify fork ancestry and retained tags, then run `make release-patch`
   from clean synchronized `main` to publish `antex-v0.0.1`.

Do not mark PLAN.md phases complete from partial evidence. Phase 10 requires the
real release and supported-platform artifacts, not just scripts or mocks.

## Validation commands

Remote checkout: `/home/antonmoss/antex-work/validation-host`.
Private tools: `/home/antonmoss/antex-tools`.

```sh
rsync -a --delete --exclude target/ \
  antex-rs/ \
  deimos.vla.yp-c.yandex.net:/home/antonmoss/antex-work/validation-host/antex-rs/

python3 scripts/antex-remote.py \
  --host deimos.vla.yp-c.yandex.net \
  --checkout /home/antonmoss/antex-work/validation-host \
  --tools /home/antonmoss/antex-tools \
  --jobs 4 \
  env ANTEX_BWRAP=/home/antonmoss/antex-tools/bin/bwrap just test
```

Do not use or reset `/home/antonmoss/antex-work/codex`; it is an older dirty
checkout. Run Rust builds, tests, Clippy, and generation only on deimos. Run
local `just fmt` after code changes. Do not rerun tests after final `just fix`
and `just fmt` in a slice.
