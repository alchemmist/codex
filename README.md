# Antex

Antex is an independent, terminal-first coding agent written in Rust. It grew from
an [OpenAI Codex](https://github.com/openai/codex) fork and preserves that history
and the Apache-2.0 license. Its target architecture is a small provider-neutral
kernel with local tools and out-of-process extensions. The first provider will
support ChatGPT subscriptions through the OpenAI adapter.

Antex is currently being extracted from the existing runtime. The migration is
tracked in [PLAN.md](PLAN.md), with verified baseline behavior and remaining gates
in the [migration matrix](migration/feature-matrix.md). Antex `0.0.1` has not been
released. The current local `0.0.14` binary remains the daily-driver fallback.

## Current runtime behavior to preserve

- The TUI uses the terminal palette and updates the composer, conversation history, plans, and diffs immediately when the terminal theme changes.
- The configurable startup cockpit shows its exact build commit, rotates feature tips, and includes two animated ant mascot skins. Its legacy branding will change during extraction.
- `Ctrl+S` stashes the current prompt draft, persists it across restarts, and restores it on the next press.
- `/subagents <prompt>` explicitly enables subagents for one request; `/subagents` arms them for the next prompt.
- `/statusline` can show the number of active subagents, while `/agents` opens an overview of their work.
- Fast mode is process-local, resets to standard on every start or resume, and shows `⚡` in the status line while active.
- `/cd <path>` changes the current session's working directory without restarting Codex.
- `/todo` shows the complete plan, while a compact adaptive list of active items remains visible at the bottom of the TUI.
- `/workflow` runs configurable Python workflows with persistent state, per-agent models and reasoning, and parallel-agent support. The PR babysitter keeps CI and review feedback moving with fresh repair agents and forbids Quality Graph ignores.
- `/tmux-command-log` creates a separate tmux window containing Codex commands and their output.
- `/context` summarizes the model-visible context, while `/system-prompt` opens the complete latest logical model request in Neovim inside a new tmux window.
- `/dump` exports the full conversation to a responsive HTML file styled like [alchemmist.xyz](https://alchemmist.xyz), with tool activity collapsed between messages.
- The composer and submitted user messages share a cyan vertical rail, making prompts easy to find throughout the conversation.
- Vim editing supports Insert, Normal, character/line/block Visual modes, Russian keyboard aliases, system-clipboard yanks, and terminal-native selection colors.
- Prompts interrupted before work begins return to the editor; later interruptions are shown without a noisy error message.
- Force pushes always require an explicit Yes or No selection in the TUI.
- Fixes include tmux pane resize redraws, focus-related flickering, and a stable `Working` animation.
- The root `Makefile` installs a local build or downloads ready-made macOS and Linux releases.

## Development

The repository still contains the legacy runtime during migration. Compiled
validation runs through GitHub Actions; local builds are paused. Local inventory
and script checks are available without compiling:

```shell
make migration-baseline
make test-migration
make migration-smoke BASELINE_BINARY=/absolute/path/to/codex
```

The existing `make install-local`, `make install-mac`, and `make install-linux`
targets still install Codex-derived binaries. In particular, `install-local`
overwrites the installed `codex`; keep the accepted local `0.0.14` fallback intact
while migrating. These targets will install only `antex` at cutover.

Clean macOS arm64 and Linux x86_64 builds and performance gates will run through
GitHub Actions in release preflight. The first Antex release will be `0.0.1`, tagged
`antex-v0.0.1`, with an independent version cycle.
