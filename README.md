# Antex

Antex is an independent, terminal-first coding agent written in Rust. It grew
from an [OpenAI Codex](https://github.com/openai/codex) fork and retains that Git
history and the Apache-2.0 license.

The active product lives in `antex-rs/` and consists of seven production crates:
a provider-neutral kernel, local runtime, OpenAI subscription adapter, terminal
UI, extension protocol and host, and the CLI composition root. Legacy Codex
product infrastructure is not part of the workspace.

Antex currently provides:

- ChatGPT subscription login and Responses streaming through the OpenAI adapter;
- `read`, `write`, `edit`, and sandboxed `shell` tools;
- append-only sessions, resume, fork, compaction, project instructions and skills;
- an inline terminal UI with Markdown, diffs, images, themes, Vim editing and
  persistent prompt stash;
- capability-sandboxed out-of-process extensions and a stdio MCP bridge;
- an optional tmux command-log extension installed explicitly with
  `antex extensions install tmux-log`;
- restartable project Python workflows installed with
  `antex extensions install workflows`;
- explicit, non-destructive `antex migrate codex` support for compatible data.

The first public release will be `0.0.1`. Until its release gates pass, the
maintainer's installed Codex `0.0.14` remains the fallback and uses a separate
`~/.codex` home. Antex uses `~/.antex` and never imports legacy data implicitly.
The release artifact contains one self-contained `antex` executable. Optional
extensions are materialized from it only when explicitly installed and are not
required for the TUI or base agent loop.

## Development

Rust builds and tests are run on the configured Linux executor:

```shell
just fmt
just test
just fix
python3 scripts/antex-baseline.py --check
```

Run the development frontend with `just antex`. Install a release build without
touching `codex` using `make install-local`.

The migration plan and current acceptance gates are recorded in [PLAN.md](PLAN.md)
and [HANDOF.md](HANDOF.md).
