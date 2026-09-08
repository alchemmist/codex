# Antex Migration Plan

## 1. Mission

Transform this repository from a continuously synchronized OpenAI Codex fork into **Antex**: an independent, minimal, terminal-first coding agent written in Rust.

Antex keeps this repository, its Git history, and its visible GitHub fork relationship to OpenAI Codex. It reuses proven code where that code serves the target product, extracts it behind smaller interfaces, and deletes the remaining Codex product machinery once no Antex path depends on it.

The migration is complete only when the repository builds and releases Antex without upstream Codex versioning, app-server infrastructure, enterprise integrations, V8, or unused compatibility surfaces.

## 2. Non-negotiable product decisions

1. **Language:** Rust remains the implementation language.
2. **Repository:** development stays in this repository; preserve Git ancestry and GitHub's fork relationship.
3. **Product:** the user-facing product, binary, configuration, release artifacts, and documentation are named `antex` / **Antex**.
4. **Philosophy:** minimal kernel, powerful extensions, terminal-first UX, no enterprise breadth in the default product.
5. **Providers:** the kernel is provider-neutral. The first adapter supports ChatGPT Plus/Pro through OpenAI Codex OAuth and Responses transport. Additional subscriptions and API-key providers attach through the same seam.
6. **Release independence:** Antex versions never derive from an upstream Codex version. The first public Antex release is `0.0.1` with tag `antex-v0.0.1`.
7. **Upstream history:** retain all existing `rust-v*` and `alchemmist-v*` tags. Never rewrite or delete them. Stop automated upstream synchronization.
8. **Platforms for 0.x:** officially support Apple Silicon macOS and x86_64 GNU/Linux. Other targets are out of scope until the kernel is stable.
9. **Daily-driver safety:** keep the current local `0.0.14` binary installed as `codex` as the fallback until Antex passes the cutover criteria. A published GitHub Release for this fallback is not required.
10. **Migration style:** extraction-and-deletion in vertical slices. Every migration commit leaves the active Antex path buildable and tested.

## 3. Success criteria and budgets

The migration is done when all of the following are true:

- `antex` is the only shipped executable.
- Login, model turns, built-in tools, sessions, and the TUI run in that process;
  no app-server, daemon, IPC service, or companion executable is required.
- A clean checkout builds with Cargo and does not build V8, app-server, daemon, cloud tasks, voice, realtime, desktop, or enterprise policy code.
- The final workspace has at most 10 production crates, excluding a dedicated test-support crate.
- The production Rust source is at most 100,000 lines; `antex-core` is at most 12,000 lines.
- No production Rust module exceeds 800 lines. Prefer modules below 500 lines.
- The stripped release binary is at most 40 MiB on each supported target.
- Warm startup reaches an editable prompt within 150 ms, excluding authentication or model-catalog network refresh.
- Idle resident memory is below 60 MiB on macOS and Linux.
- The complete local test suite finishes within 5 minutes on the maintainer's machine after a warm build.
- The default model receives only the system prompt, bounded project instructions/skills, conversation context, and tools actually enabled for the session.
- The built-in system prompt is at most 4 KiB. Multi-step orchestration belongs
  to explicit workflows rather than the kernel prompt or agent loop.
- A maintainer can understand the agent loop, tool host, and session store in one focused afternoon.
- `make install-local`, `make install-mac`, and `make install-linux` install `antex` without any `codex-*` companion binary.
- `make release-patch` from development version `0.0.0` creates `antex-v0.0.1` and publishes working macOS/Linux artifacts.

Treat these as hard acceptance gates. If a budget is missed, reduce scope or deepen a module before declaring migration complete.

## 4. Terminology

Use these names consistently throughout code and documentation:

- **Antex:** this product and its runtime.
- **OpenAI adapter:** Antex's adapter for ChatGPT OAuth, model discovery, and Codex Responses transport.
- **Codex compatibility revision:** an adapter-internal protocol/header value needed for OpenAI endpoint compatibility. It is not the Antex version and is never displayed as one.
- **Legacy runtime:** the current Codex-derived execution path during migration.
- **Kernel:** provider-neutral agent loop and its messages/events.
- **Extension:** an out-of-process program connected through the Antex extension protocol.

Do not blindly replace every occurrence of `codex`. Names referring to the OpenAI Codex provider or its wire protocol remain accurate. Product names, filesystem paths, crate names, environment variables, UI copy, and release assets become Antex names.

## 5. Target architecture

Keep the final workspace deliberately small:

```text
antex-rs/
  Cargo.toml
  cli/                 # composition root and `antex` executable
  core/                # provider-neutral agent kernel and event model
  runtime/             # tools, permissions, config, sessions, compaction
  provider-openai/     # ChatGPT OAuth and Codex Responses adapter
  tui/                 # terminal presentation and input
  extension-protocol/  # versioned JSON-RPC types
  extension-host/      # subprocess lifecycle and tool/command bridging
  test-support/        # fakes and end-to-end harnesses
```

Do not create a crate for every concept. Config, sessions, compaction, built-in tools, and permissions are private modules inside `antex-runtime` until a second real adapter proves that a crate seam is needed.

### 5.1 Dependency direction

```text
antex-cli
  ├── antex-tui
  ├── antex-runtime
  ├── antex-provider-openai
  └── antex-extension-host

antex-tui ───────────────> antex-core
antex-runtime ───────────> antex-core
antex-provider-openai ───> antex-core
antex-extension-host ────> antex-core + antex-extension-protocol

antex-core ──────────────> no other Antex crate
```

`antex-core` must not depend on terminal rendering, filesystems, config formats, OAuth, HTTP clients, Git, tmux, MCP, session serialization, or extension transports.

### 5.2 Agent kernel interface

The kernel owns:

- provider-neutral messages and content blocks;
- tool definitions and validated tool calls;
- a deterministic assistant → tools → assistant loop;
- cancellation, steering, and follow-up queues;
- bounded context preparation hooks;
- a small event stream for presentation and persistence.

Use one concrete `Agent` implementation. Do not add a trait for the agent itself.

The external interface should be equivalent to:

```rust
pub struct Agent<P> { /* private */ }

impl<P: ModelProvider> Agent<P> {
    pub fn start(&mut self, input: TurnInput) -> AgentRun;
}

pub struct AgentRun {
    pub events: tokio::sync::mpsc::Receiver<AgentEvent>,
    pub commands: tokio::sync::mpsc::Sender<AgentCommand>,
}

pub enum AgentCommand {
    Steer(UserInput),
    FollowUp(UserInput),
    Interrupt,
}
```

The exact type names may change during extraction, but the information surface must not grow. Callers submit input, observe events, and send control commands; they do not manipulate kernel state directly.

### 5.3 Model provider seam

The provider module hides authentication, transport, model catalog, request conversion, retries, and provider-specific errors behind:

```rust
pub trait ModelProvider: Send + Sync {
    fn models(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send;
    fn stream(
        &self,
        request: ModelRequest,
    ) -> impl Future<Output = Result<ModelStream, ProviderError>> + Send;
}
```

Use static or enum dispatch for built-in adapters so the trait can use native async return types without `async_trait`. Out-of-process provider extensions use the extension protocol and do not require trait objects.

The OpenAI adapter owns all legitimate Codex-specific concepts. No other crate may refer to ChatGPT access tokens, Codex endpoint paths, OpenAI account IDs, OpenAI model metadata, or the compatibility revision.

### 5.4 Tool host seam

The runtime owns a registry of tools. A tool exposes metadata, a bounded input schema, and execution:

```rust
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    fn execute(
        &self,
        call: ToolCall,
        context: ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>>;
}
```

The boxed future is intentional because tools are dynamically registered by extensions and MCP. Do not use `async_trait`.

Built-in tools for `0.0.1`:

- `read`;
- `write`;
- `edit`;
- `shell`.

File search and listing use `shell` initially. Add a dedicated tool only when measurements show a model-quality or safety benefit.

### 5.5 Session store

Use append-only JSONL under `~/.antex/sessions/<workspace-key>/`.

Each record has:

- schema version;
- record ID and optional parent ID;
- session ID;
- timestamp;
- record kind;
- provider-neutral payload.

Required record kinds are `session`, `user`, `assistant`, `tool_call`, `tool_result`, `summary`, `branch`, and `extension`. Branching uses IDs and parent IDs rather than copying an entire transcript.

The store interface is `create`, `open`, `append`, `active_path`, `branch`, and `list`. Persistence details remain private. Writes are atomic at the record level and fsynced at turn completion, not after every token delta.

### 5.6 Extension protocol

Extensions are executables launched as child processes. This keeps the Rust kernel small and lets extensions be written in Rust, Python, TypeScript, or any language that speaks JSON-RPC 2.0 over newline-delimited stdio.

Protocol v1 methods:

- host → extension: `initialize`, `tool/call`, `command/run`, `event/notify`, `shutdown`;
- extension → host responses: manifest, tool/command definitions, results;
- extension stderr: human-readable logs only;
- extension stdout: protocol messages only.

The initialization payload contains protocol version, Antex version, session ID, cwd, and granted capabilities. It never contains provider credentials or unrestricted config. Extension tools execute only with their declared permissions.

Protocol v1 supports:

- registering model-callable tools;
- registering slash commands;
- observing bounded lifecycle events;
- persisting extension records in the session;
- publishing one-line status and bounded text panels.

Arbitrary custom TUI widgets and in-process dynamic libraries are outside protocol v1.

## 6. Feature disposition

### 6.1 Keep in the product path

- ChatGPT Plus/Pro OAuth and token refresh;
- OpenAI Codex Responses transport and dynamic model catalog;
- streaming responses and tool calls;
- terminal-native TUI, markdown, diffs, images, clipboard paste;
- current ant mascot and startup identity;
- Vim editor behavior and Russian layout aliases;
- project instructions and skills;
- prompt queueing, interruption, and stash behavior;
- session resume, fork, compaction, and `/cd`;
- macOS and Linux local execution/permissions;
- release downloads and update checks for Antex.

### 6.2 Move out of the kernel

Implement these as first-party extensions or presentation modules before deleting their legacy implementations:

- MCP client → `antex-ext-mcp`;
- explicit subagents and `/agents` → `antex-ext-agents`;
- Python workflows → `antex-ext-workflows`;
- tmux command logging → `antex-ext-tmux-log`;
- PR babysitter → workflow package;
- `/dump` → transcript export extension;
- `/context` and `/system-prompt` → diagnostic extension;
- persistent TODO panel → plan extension.

Prompt stash and Vim editing remain in `antex-tui` because they directly own editor state rather than agent behavior.

### 6.3 Delete after replacement paths are green

- app-server, app-server daemon/client/test-client, compatibility negotiation, and desktop handoff;
- cloud tasks and cloud config;
- connectors/apps and marketplace product integration;
- guardian, auto-review, enterprise managed policy, and organization-specific controls;
- feedback upload, analytics, OpenTelemetry, rollout tracing, and remote diagnostics;
- voice, realtime WebRTC, audio host, and realtime conversation;
- Bedrock/AWS, Ollama, LM Studio, and bundled non-OpenAI providers;
- code mode, code-mode host/runtime/protocol, V8 POC, and every `rusty_v8` dependency;
- remote exec server, UDS daemon transport, and background server management;
- memories and history-note extensions;
- embedded MCP server;
- Windows sandbox/service and Windows-only release paths;
- Bazel build, Bazel locks, upstream CI scaffolding, DotSlash tools, and upstream release automation;
- NPM package detection and upstream Codex update checks;
- unused migrations, legacy protocol versions, sample binaries, and test-support crates whose production path is gone.

Do not delete by directory name alone. A deletion is allowed only when `cargo tree -i <crate>` shows no Antex production dependency and the replacement behavior has an acceptance test.

## 7. Execution protocol

The implementing agent must:

1. Create a migration branch named `antex` from the current `main`.
2. Convert every numbered phase below into a synchronized checklist before editing.
3. Commit each coherent vertical slice separately with a one-line lowercase English message.
4. Keep changes below 800 lines per non-mechanical commit and below 500 lines for complex logic.
5. Use `git mv` when code is retained under a new name so history remains traceable.
6. Prefer replacement tests at the new interface over tests that reach into old internals.
7. Delete legacy code in the same commit that removes its final caller, or in the immediately following mechanical deletion commit.
8. Avoid permanent `legacy`, `compat`, or boolean feature branches in the new architecture. Temporary adapters must carry a removal phase and completion criterion in this plan.
9. Keep the current local `0.0.14` binary installed separately while developing; do not publish an Antex tag before Phase 10.
10. Stop and repair the current phase if the Antex binary, its focused tests, or the migration fixtures fail. Do not accumulate broken phases.
11. Do not compile on the maintainer's Mac. The maintainer's remote Linux build host is now available and is the primary executor for builds, tests, Clippy, and generation. Use an isolated checkout of `antex` there. Do not run iterative heavy validation through GitHub Actions; the migration workflow is manual-only. Retain the final platform/release gates. Continue until the complete definition of done is met, recording evidence rather than treating intermediate slices as completion.

## 8. Migration phases

### Execution checklist

Each checkbox requires the corresponding phase's completion criterion, not merely
the presence of implementation files. Evidence is tracked in `migration/`.

- [ ] Phase 0 — Freeze the behavioral baseline.
- [ ] Phase 1 — Establish Antex identity without releasing.
- [ ] Phase 2 — Create the provider-neutral kernel.
- [ ] Phase 3 — Extract the OpenAI provider adapter.
- [ ] Phase 4 — Build the minimal local runtime.
- [ ] Phase 5 — Replace the app-server TUI path.
- [ ] Phase 6 — Add the extension host.
- [ ] Phase 7 — Migrate required first-party extensions.
- [ ] Phase 8 — Delete legacy product infrastructure.
- [ ] Phase 9 — Final repository and filesystem rename.
- [ ] Phase 10 — Reset and ship the independent release cycle.

### Phase 0 — Freeze the behavioral baseline

#### Work

- Record the current commit, upstream base `0.153.4`, local fork version `0.0.14`, supported release targets, and available binary sizes. Clean platform builds and build-time/startup/memory measurements are deferred to Phase 10 GitHub Actions release preflight by the maintainer's decision; they do not block migration work.
- Add black-box fixtures covering:
  - ChatGPT login token loading and refresh with a fake OAuth server;
  - one assistant response without tools;
  - one shell tool round trip;
  - interrupt and steering;
  - session save/resume/fork;
  - compaction;
  - image input;
  - narrow/wide TUI snapshots;
  - theme switching;
  - model catalog refresh.
- Capture the exact fork-specific features that must survive in a migration matrix with one owner phase for each.
- Add a script that reports workspace crates, reverse dependencies, production LOC, dependency nodes, release binary size, and forbidden dependency presence.

#### Completion criterion

The baseline report is reproducible from one command, all fixtures pass against the legacy binary, and every feature in the current README has a keep, extension, or delete disposition.

### Phase 1 — Establish Antex identity without releasing

Temporary adapter: `cli/src/antex_entry.rs` and `cli/src/antex.rs` reuse the current
CLI/runtime during extraction. The entry point resolves only `ANTEX_HOME`, then
re-execs with internal legacy home variables pointing to that directory before
any runtime initialization. `CODEX_HOME` is not an Antex user override. Remove this
bridge in Phase 5 when the composition root connects directly to the new kernel
and no legacy runtime reads these variables. This adapter is not the final CLI
surface. Terminal headers, status cards, startup identity, and the default title
now use Antex; remaining command/help/log surfaces still require the Phase 1 audit.

Remote validation on deimos: 4,340 TUI tests passed (2 existing skips), 295 config
tests passed, config schema regenerated, and scoped TUI Clippy passed. The narrow
startup snapshot exercises a 32-column terminal after the shorter title changed
the mascot fit boundary. Validation uses an isolated checkout and private toolchain
through `scripts/antex-remote.py`; no local compilation or installation is needed.

#### Work

- Rename user-facing copy, package metadata, default terminal title, logs, temporary-file prefixes, and documentation from alchemmist Codex to Antex.
- Introduce an `antex` binary and composition root. During this phase it may still call the legacy runtime.
- Change the default config/data home from `~/.codex` to `~/.antex`.
- Keep `CODEX_HOME` only as an explicitly documented legacy import source; introduce `ANTEX_HOME` as the active override.
- Rename build environment variables from `CODEX_*` and `ALCHEMMIST_*` to `ANTEX_*` where they describe this product.
- Add `antex migrate codex --dry-run` and `antex migrate codex` commands. Migration copies only supported config keys, skills references, MCP definitions, prompt stash, and sessions. It reports every skipped key and never edits `~/.codex`.
- Keep the repository's Apache-2.0 license and add a NOTICE describing its Codex ancestry. Preserve MIT notices for any Pi code copied rather than merely studied.
- Rewrite README as an independent product README that clearly states Antex grew from the OpenAI Codex fork.

#### Completion criterion

`cargo run --bin antex` starts an Antex-branded UI using `~/.antex`; the legacy `~/.codex` tree remains untouched; dry-run migration reports deterministic actions; no user-visible surface calls the product Codex.

### Phase 2 — Create the provider-neutral kernel

During extraction the new crates live in the Cargo-only workspace at
`codex-rs/antex/`, independent of the legacy Cargo/Bazel dependency graph.
After the legacy production paths are removed, promote this workspace to the
repository's Rust root and flatten its crate paths as part of Phases 8–9 using
`git mv`. This staging arrangement does not relax the kernel or TUI isolation
criteria.

Kernel progress: the standalone `Agent::start` now drives validated sequential
tool calls, bounded steering/follow-up queues, interruption, pre-stream retries,
an immutable context hook, and terminal events. Twelve tests passed on deimos,
including malformed/unknown tools, truncated streams, multi-tool cancellation,
scope reset on follow-up, and rejection of overlapping runs; scoped Clippy passed.
The legacy adapter and application cutover remain incomplete, so Phase 2 is not
marked complete.

#### Work

- Create `antex-core` with provider-neutral message, tool, event, command, usage, and error types.
- Implement the concrete deterministic agent loop using a fake provider and fake tools first.
- Support sequential assistant/tool iterations, multiple tool calls, cancellation, steering, follow-ups, bounded retries, and terminal completion.
- Establish one bounded context-preparation hook. It receives an immutable transcript and returns the model input for the next request.
- Ensure UI-only and extension-only events never enter model context unless an explicit converter produces a bounded model message.
- Add hard size caps for every injected context fragment and tool result.
- Keep the legacy runtime behind one temporary adapter that translates legacy events into `AgentEvent`. No TUI code may call deeper legacy types after this phase.

#### Completion criterion

Integration tests drive the same `Agent::start` interface used by the application and cover the complete fake-provider loop. `antex-core` has no dependency on Codex core, app-server, TUI, config, HTTP, filesystem, or session crates.

### Phase 3 — Extract the OpenAI provider adapter

Progress: the independent adapter now provides HTTP/SSE Responses, bounded replay
of reasoning/message items, quota events, dynamic model discovery, private atomic
credential storage, serialized refresh, account selection/logout, browser PKCE,
and device-code login. Fourteen provider tests and twelve kernel tests passed on
deimos with fake credentials; scoped Clippy passed. Live subscription acceptance,
identification probing, and the remaining transport failure/reconnect fixtures
are still required. No live credential has been used in automated validation.

#### Work

- Move the smallest necessary ChatGPT OAuth, secure credential storage, refresh, account selection, Codex Responses serialization/stream parsing, usage-limit parsing, and model-catalog code into `antex-provider-openai`.
- Expose only provider-neutral models and streamed events to `antex-core`.
- Separate `ANTEX_VERSION` from `OPENAI_CODEX_COMPAT_REVISION`.
- Initialize compatibility revision to `0.153.4`, the last adopted upstream baseline. Keep it private to the adapter.
- Verify whether Pi-compatible Antex identification works without an emulated Codex version. If the endpoint accepts it, remove the compatibility revision before `0.0.1`; otherwise keep the private pin with a contract test and one documented update procedure.
- Add mock-server contract tests for login, refresh, model discovery, normal streaming, tool calls, reasoning, rate limits, malformed streams, reconnect, and cancellation.
- Never use live credentials in automated tests.

#### Completion criterion

The Antex kernel completes real manual turns through a ChatGPT Plus/Pro subscription, all provider contract tests pass offline, and no crate outside `antex-provider-openai` imports an OpenAI/Codex transport or auth type.

### Phase 4 — Build the minimal local runtime

Progress: `antex-runtime` now owns capability-scoped file operations, newline-safe
unique edits, sandboxed shell execution, one-shot approvals, bounded image
normalization, JSONL sessions, parent-linked forks, torn-tail recovery, and
checkpoint-based compaction. Runtime config and frozen project/skill context are
connected to the independent `antex exec` path. Compaction preserves the latest
request and complete recent tool exchanges, and records summaries without
rewriting committed records. Image-heavy histories are compacted before sampling.

Remote evidence includes Linux runtime and kernel integration tests, provider
contract tests with fabricated credentials, CLI resume tests, and scoped Clippy.
The Linux sandbox uses privately built Bubblewrap 0.12.0 plus seccomp, not an
unsandboxed fallback. The macOS runtime branch passed cross-target compilation
on deimos; real macOS execution remains a release gate. A preliminary stripped
Linux CLI build was 11,259,128 bytes (10.74 MiB), before the new TUI and extensions.

The manual ChatGPT device-login attempt reached the official verification flow,
but expired without user authorization. No live subscription turn is claimed;
request a fresh code when the maintainer is available. The isolated preflight
home is outside the checkout and does not read or modify the installed fallback.

#### Work

- Create `antex-runtime` and port the four built-in tools: read, write, edit, shell.
- Preserve the existing apply-patch correctness, output truncation, image normalization, and shell process handling while collapsing their interfaces.
- Implement three permission profiles: `read-only`, `workspace`, and `full`. Keep approval decisions in runtime events so the TUI only presents them.
- Support Seatbelt on macOS and the smallest viable Linux sandbox adapter. Full mode bypasses the sandbox explicitly.
- Implement the JSONL session store, branching, resume, prompt history, and explicit compaction.
- Implement proactive auto-compaction through a config token limit without embedding model policy in the kernel.
- Load `AGENTS.md`, skills, and bounded project context in the runtime context-preparation module.
- Add config parsing for only settings used by Antex. Unknown migrated Codex keys produce diagnostics rather than silent behavior.

#### Completion criterion

Antex can complete file-editing tasks, resume and fork sessions, survive interruption, compact context, and enforce each permission profile without using legacy core, rollout, state, config, exec-server, or sandboxing types.

### Phase 5 — Replace the app-server TUI path

Checkpoint 2026-09-07: the migration is paused at a working explicit `antex tui`
entry point, not at release readiness. The historical progress notes below
describe successive slices; the current consolidated status, remaining gaps,
validation commands, and remote environment are recorded in `HANDOF.md`.
Model/session/fork pickers, image/clipboard controls, persisted rich stash,
complete-draft Vim undo, paste-burst handling, durable input queues, transcript
paging, and session themes are now connected. The final focused Linux run
passed 893 tests across the six Antex packages (one skip); an unauthenticated
tmux smoke exercised help/Escape, draft stash/restore, status, and clean exit.
Live subscription use and complete baseline parity remain acceptance gates.

The first retained presentation slice moves the original mascot renderer,
palette, and tests into `antex-tui` with `git mv`. During extraction the old TUI
includes these same source files through narrow source-path bridges; no legacy
crate becomes a dependency of the new workspace. Remove these bridges when the
old TUI is retired. Existing styles and snapshots remain the visual authority.

The shared-source extraction now also includes keymap resolution, Unicode
wrapping, terminal hyperlinks, cursor-safe terminal drawing, scrollback
insertion, and the original textarea with Vim/Russian commands. Textarea
implementation is split into editing, elements, mode, navigation, painting,
and Vim-input modules; its top-level module is below 800 lines. The independent
TUI suite passes 387 tests on deimos in 4.343 seconds (one pre-existing ignored
test). Renamed external snapshots were compared against the original payloads;
no rendering changes were accepted. This is component validation, not the
direct-kernel frontend acceptance gate: the application/composer orchestration,
runtime integration, and remaining presentation extraction are still pending.
The legacy TUI also passes all 4,352 selected tests after extraction (six
pre-existing skips; 22.410 seconds after compilation) on deimos.

The explicit `antex tui` command now runs the terminal directly against
`AgentRun`, with a runtime-owned durable conversation projection. Its first
real-kernel terminal fixture covers submission, streaming, persistence, and
inline scrollback. Additional fixtures cover deny-by-default approvals,
explicit one-shot allowance, and cancellation/draining after terminal EOF.
Interrupted input queues are stored separately from model history and reconciled
against subsequently committed user messages during resume. Both console and
interactive paths share this persistence adapter. A clean-config tmux smoke on
deimos exercised editable startup without network, `/status`, narrow resize,
and `/quit` (exit 0), using an isolated unauthenticated home. Retained ant startup
rendering is now attached to the direct frontend. Original Markdown, syntax
highlighting, palette/probe, table, hyperlink, and diff presentation modules are
now shared by both paths. The direct frontend renders Markdown and explicitly
proposed edit/write previews, bounds displayed content, and coalesces streaming
redraws while keeping keyboard input immediate. Large Markdown and diff modules
were split along parser/rendering responsibilities. Deimos validation: 761 TUI
and CLI tests passed in 4.880 seconds, and 4,371 legacy TUI/terminal-detection
tests passed in 21.747 seconds (six existing skips). Scoped Clippy completed;
unused-import diagnostics were disabled for that invocation because retained
source-path bridges still have consumers in the legacy crate. Picker UX,
clipboard plumbing, complete stash controls/persistence, and remaining baseline
interactions still need completion before replacing the default entry.

#### Work

- Create or reduce `antex-tui` so it talks directly to `AgentRun` events and commands.
- Port only the proven presentation modules needed for:
  - startup panel and both ant skins;
  - composer, Vim modes, Russian aliases, stash, clipboard images;
  - user/assistant messages;
  - reasoning and tool activity folding;
  - markdown and diffs;
  - approvals and structured questions;
  - model picker, `/cd`, session resume/fork, and status line;
  - terminal palette switching and resize reflow.
- Preserve inline terminal scrollback as the default. Do not introduce a permanent alternate-screen dependency.
- Keep the startup ant animation bounded and one-shot.
- Replace app-server requests, notifications, replay snapshots, and compatibility negotiation with direct kernel/runtime events.
- Delete presentation paths for desktop handoff, connector catalogs, enterprise notices, cloud tasks, remote sessions, background daemon command center, voice, and unused setup flows.

#### Completion criterion

All baseline TUI fixtures pass through the direct kernel path; `antex-tui` has no dependency on app-server client/protocol, daemon, legacy Codex core, cloud, connectors, or remote execution crates; narrow tmux panes and theme changes remain stable.

### Phase 6 — Add the extension host

Checkpoint 2026-09-07: `antex-extension-protocol` contains the bounded wire types,
envelope/permission validation, and four passing tests. This is a scaffold only:
the extension host, process lifecycle, invocation-origin enforcement, SDK, and
Rust/Python conformance fixtures have not been implemented.

#### Work

- Implement protocol v1 in `antex-extension-protocol` with serde types and strict version negotiation.
- Implement `antex-extension-host` with child lifecycle, bounded messages, cancellation, timeouts, stderr logging, restart policy, and capability grants.
- Discover global extensions from `~/.antex/extensions/` and project extensions from `.antex/extensions/` only after project trust is granted.
- Add one conformance fixture in Python and one in Rust. Both register a command and a tool and persist an extension session record.
- Provide a tiny extension SDK reference implementation for Python. It may be a separate directory/package but not an embedded Python runtime.
- Fail one extension independently; an extension crash must not terminate the agent or corrupt the session.

#### Completion criterion

The same conformance suite passes for Rust and Python extension fixtures; malformed, oversized, stalled, and crashing extensions fail closed; disabling all extensions removes their runtime cost and tool definitions.

### Phase 7 — Migrate required first-party extensions

#### Work

Migrate in this order:

1. `antex-ext-mcp` for the user's existing MCP-heavy workflow.
2. `antex-ext-tmux-log` for visible command execution.
3. `antex-ext-agents` for explicit `/subagents` and `/agents`; ordinary prompts never spawn subagents.
4. `antex-ext-workflows` using the existing Python workflow contract through the extension host.
5. transcript export, context inspection, system-prompt inspection, and TODO presentation.
6. PR babysitter as a packaged workflow rather than kernel behavior.

Each extension owns its slash commands, status entries, persisted extension state, tests, and failure isolation. Shared functionality moves into runtime only after two extensions demonstrate the same need.

#### Completion criterion

Every current fork feature marked "move out of kernel" works through an extension, its legacy implementation has no callers, and disabling the extension removes its tools, commands, context, and background activity.

### Phase 8 — Delete legacy product infrastructure

#### Work

Delete in dependency-safe batches:

1. code-mode/V8 and companion host;
2. cloud tasks/config, connectors/apps, and marketplace paths;
3. voice, realtime, alternative bundled providers, and AWS code;
4. guardian, enterprise policy, analytics, telemetry, feedback, and memories;
5. remote exec, UDS, app-server daemon/client/protocol, desktop paths, and compatibility layers;
6. obsolete state, rollout, history, config, tool, and test-support crates;
7. Windows-only production paths;
8. Bazel, upstream workflows/assets, npm update logic, samples, and unused documentation.

After every batch:

- run reverse-dependency checks;
- remove workspace members and dependency declarations;
- run focused tests and the workspace suite;
- compare binary size, build time, dependency nodes, LOC, startup, and memory with the Phase 0 baseline;
- update the migration matrix.

Do not preserve empty adapter crates or pass-through modules for historical neatness. Git already preserves history.

#### Completion criterion

The workspace contains only target-architecture crates, every forbidden dependency check is green, `rg` finds no legacy product identifiers outside migration fixtures/NOTICE/OpenAI adapter terminology, and the quantitative budgets in Section 3 pass.

### Phase 9 — Final repository and filesystem rename

#### Work

- Rename `codex-rs/` to `antex-rs/` with `git mv` after legacy crates are gone.
- Rename all remaining packages to `antex-*` and the executable to `antex`.
- Rename config paths, logs, cache files, session files, temp prefixes, terminal client names, archive names, and environment variables.
- Remove the `codex` binary alias. Users who need the old product use the retained `alchemmist-v0.0.14` release.
- Rename the GitHub repository from `alchemmist/codex` to `alchemmist/antex`. Keep the GitHub fork relationship and update the local `origin` URL.
- Keep an `upstream` remote only as optional historical reference. Remove all scripts and documentation that instruct routine syncing or merging from it.
- Update README, NOTICE, contribution instructions, badges, release URLs, and install scripts.

#### Completion criterion

A fresh clone from `alchemmist/antex` contains no active product path named Codex, `antex --version` reports an Antex development version, and old Codex data is accessed only through the explicit migration command.

### Phase 10 — Reset and ship the independent release cycle

#### Version model

- Make `antex-rs/Cargo.toml` workspace package version the canonical product version.
- Set the development baseline to `0.0.0` before the first release.
- Remove `FORK_VERSION` and `ALCHEMMIST_FORK_VERSION`.
- The first `make release-patch` changes the workspace version and internal package lock entries from `0.0.0` to `0.0.1`.
- Product version is used for CLI display, TUI display, artifacts, update checks, and extension negotiation.
- OpenAI compatibility revision remains private to `antex-provider-openai` and never participates in SemVer or release checks.

#### Release commands

Keep these commands:

```text
make install-local
make install-mac
make install-linux
make release-patch
make release-minor
make release-major
```

They operate only on Antex.

#### Release script

Replace `scripts/release-fork.sh` with `scripts/release-antex.sh`:

- require clean `main` matching `origin/main`;
- read the canonical workspace version;
- bump SemVer without updating third-party dependency versions;
- refresh only internal package entries in `Cargo.lock`;
- run release preflight;
- commit `release <version>`;
- create annotated tag `antex-v<version>`;
- atomically push `main` and the tag;
- never fetch, compare, or mention an upstream Codex version.

#### GitHub Actions

Replace `alchemmist-release.yml` with `antex-release.yml`:

- trigger only on `antex-v*.*.*`;
- validate tag against Cargo workspace version;
- build `antex` for `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`;
- package exactly one binary;
- produce `antex-<target>.tar.gz` plus SHA-256;
- strip the binary and ad-hoc sign the macOS binary;
- create GitHub Release `Antex v<version>`;
- mark it latest;
- include generated release notes;
- contain no V8 setup step.

#### Installer

Replace fork installer naming and behavior:

- default repository: `alchemmist/antex`;
- temp prefix: `antex-install`;
- download `antex-<target>.tar.gz`;
- verify checksum;
- install only `~/.local/bin/antex`;
- remove every `codex-code-mode-host` expectation;
- print `Installed Antex <version>`.

#### Update check

- Query only `https://api.github.com/repos/alchemmist/antex/releases/latest`.
- Accept only tags with prefix `antex-v`.
- Store cache under `~/.antex/cache/update-version.json`.
- Show one Antex update row in the startup panel.
- Remove the upstream Codex update row, Homebrew Codex lookup, npm Codex lookup, and fork/upstream dual-version concepts.

#### First release preflight

Before creating `antex-v0.0.1`:

The maintainer deferred Phase 0 clean platform builds and quantitative measurements
to this preflight. Run them through GitHub Actions on both supported platforms and
retain the measured results as workflow artifacts. Missing measurements must not
be interpreted as passing budgets.

1. fresh-clone build on both supported platforms;
2. complete tests, Clippy, format, schema, and extension conformance;
3. manual ChatGPT Plus/Pro login and model turn;
4. manual file-editing task with tool round trip;
5. session resume and explicit Codex migration;
6. MCP, workflow, tmux log, and explicit subagent smoke tests;
7. light/dark theme and narrow tmux pane checks;
8. installer test against a draft release;
9. binary size/startup/memory/build-time budgets;
10. verify `antex --version` returns `antex 0.0.1` and no visible `0.153.4` or `0.0.14` remains.

#### Completion criterion

`make release-patch` publishes `antex-v0.0.1`; macOS and Linux installers retrieve it; the binary authenticates with ChatGPT subscription and completes the acceptance task; the UI checks only for newer Antex releases.

## 9. Verification strategy

### 9.1 Test layers

- **Kernel:** deterministic integration tests with fake provider and tools.
- **Provider:** HTTP/WebSocket fixtures for every OpenAI request and stream branch.
- **Runtime:** temporary filesystem tests for tools, permissions, sessions, compaction, and migration.
- **Extensions:** language-independent protocol conformance tests.
- **TUI:** `insta` snapshots for every visible state and PTY tests for input, resize, theme, and interruption.
- **End-to-end:** compiled `antex` against fake provider, then bounded manual subscription smoke tests.

### 9.2 Required regression scenarios

- a model can issue zero, one, or multiple tool calls;
- malformed/truncated tool arguments fail without execution;
- tool results are bounded before model injection;
- cancellation terminates provider and running tools;
- steering is delivered once and in order;
- session replay reconstructs the same active path;
- compaction preserves the latest user intent and tool state;
- provider failure never corrupts the session;
- extension failure never terminates Antex;
- a narrow terminal hides decorative content before useful content;
- theme switching keeps terminal-native transcript content readable;
- no ordinary prompt starts a subagent;
- product version cannot affect OpenAI protocol compatibility;
- update checks cannot suggest an upstream Codex release.

### 9.3 Deletion gates

Maintain a machine-readable deny list checked in CI. The final build must reject dependencies or source references to:

```text
rusty_v8
deno_core
codex-app-server
codex-cloud-tasks
codex-connectors
codex-otel
codex-realtime-webrtc
codex-voice-host
codex-windows-sandbox
```

Extend the list when additional legacy clusters are identified.

## 10. Commit and review sequence

Use this commit topology; split a line further when it exceeds repository size guidance:

1. `capture antex migration baseline`
2. `introduce antex product identity`
3. `add provider neutral agent kernel`
4. `route legacy runtime through antex events`
5. `extract openai subscription provider`
6. `add minimal antex tools and permissions`
7. `add antex jsonl sessions`
8. `connect tui directly to antex kernel`
9. `add antex extension protocol`
10. `migrate mcp extension`
11. `migrate custom antex extensions`
12. one or more `remove legacy <cluster>` commits
13. `rename repository paths for antex`
14. `establish antex release cycle`
15. `release 0.0.1`

Mechanical renames and deletions may be large. Keep behavioral changes isolated from them so review can distinguish movement from new logic.

## 11. Risks and controls

### OpenAI subscription compatibility

Risk: the Codex Responses endpoint evolves independently of Antex.

Control: isolate it in one adapter, preserve contract fixtures, expose a private compatibility revision, and document one adapter-only update procedure.

### Endless compatibility layer

Risk: the new kernel becomes a wrapper around legacy app-server types.

Control: Phase 2 forbids legacy types outside one adapter; Phase 5 deletes the TUI app-server dependency; every temporary adapter has a named deletion phase.

### Extension interface inflation

Risk: copying Pi's broad extension surface recreates a second product framework.

Control: protocol v1 contains only tools, commands, events, persistence, status, and bounded panels. Add capabilities only after two real extensions require them.

### TUI rewrite scope

Risk: preserving the current 300k-line TUI defeats minimalism.

Control: preserve behavior through black-box snapshots while porting only visible daily-driver paths. Delete setup, remote, marketplace, enterprise, desktop, and compatibility screens rather than adapting them.

### Premature release

Risk: tagging a legacy build as Antex `0.0.1` creates a false starting point.

Control: no `antex-v*` tag exists before all Phase 10 preflight checks pass. Development builds report `0.0.0+<commit>`.

### Loss of working agent

Risk: the migration interrupts daily use.

Control: develop on branch `antex`, keep the current local `0.0.14` installed as `codex`, install development Antex as `antex`, and keep their data homes separate. The maintainer explicitly accepted this local fallback without a published `0.0.14` GitHub Release.

## 12. Final definition of done

The implementing agent may declare this plan complete only after verifying every statement below:

- The GitHub repository is `alchemmist/antex` and visibly retains its Codex fork ancestry.
- The repository builds one product binary named `antex`.
- The workspace matches the target dependency direction and crate budget.
- ChatGPT Plus/Pro login and model use work through the isolated OpenAI adapter.
- The kernel is provider-neutral and independently testable.
- Four built-in tools, sessions, compaction, skills, images, permissions, and the minimal TUI work without legacy runtime code.
- MCP, explicit subagents, workflows, and tmux logging work as extensions.
- Ordinary prompts cannot start subagents unless the explicit extension command arms them.
- All deletion gates and quantitative budgets pass.
- Config and runtime state live under `~/.antex`; Codex import is explicit and non-destructive.
- Release tags, artifacts, installers, update checks, UI version, and extension negotiation use only Antex SemVer.
- `antex-v0.0.1` is published with working macOS arm64 and Linux x86_64 artifacts.
- Existing `alchemmist-v*` and upstream tags remain intact as historical provenance.
