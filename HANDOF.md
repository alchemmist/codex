# Antex handoff

Date: 2026-09-07. Branch: `antex`.

Antex is now the only Rust product workspace. The independent workspace was
promoted to `antex-rs/`; the legacy `codex-rs`, app-server, V8/code-mode, cloud,
enterprise, voice, Windows, Bazel, npm, upstream SDK and legacy release
infrastructure were removed from Git.

This is still a development checkpoint, not release readiness. No `antex-v*`
tag or GitHub Release has been published, and the GitHub repository has not yet
been renamed.

## Current architecture

`antex-rs/` contains seven production crates:

- `core` — provider-neutral deterministic agent kernel;
- `provider-openai` — ChatGPT OAuth, model discovery and Responses transport;
- `runtime` — local tools, permissions, sandbox, sessions and context;
- `tui` — direct inline terminal frontend;
- `extension-protocol` and `extension-host` — bounded out-of-process extensions;
- `cli` — the `antex` composition root.

Antex has no production dependency on legacy Codex crates. The inventory gate
reports 35,517 production Rust lines, seven crates and no forbidden dependency.

## Verified evidence

- The promoted Linux workspace passed 909 tests with one existing skip in 5.032s
  on deimos.
- Scoped and workspace Clippy run on deimos. Retained TUI presentation code still
  has dead-code and three disallowed-yellow warnings that must be removed before
  the release workflow's `-D warnings` gate.
- Runtime and extension-host cross-check for `aarch64-apple-darwin` passes on
  deimos. Full CLI cross-build needs an Apple SDK for `ring`; real macOS build and
  execution remain release gates.
- Extension host conformance passes for Rust and Python fixtures. Malformed,
  oversized, stalled and crashing extensions fail independently.
- Linux extension sandbox tests prove protocol stdin remains available while
  seccomp uses FD 3, and verify workspace/network capability isolation.
- The stdio MCP bridge passes an end-to-end host → sandbox → MCP server tool call.
- `/Users/antonmoss/.local/bin/codex` remains unchanged with SHA-256
  `be16a880b76ea5c6d4a38e61ff1f4fa86de15306078513d639cc1716df3528b2`.

## Remaining product work

- Complete live ChatGPT subscription login, model turn and tool-cycle acceptance.
- Add streamable HTTP/OAuth MCP support.
- Implement the required first-party tmux-log, explicit-agents and workflows
  extensions plus diagnostic/export packages.
- Execute extension command actions and persist/reload extension state through
  the host seam.
- Remove any newly exposed dead presentation paths as the remaining extensions are migrated.
- Finish Antex installer/release tests and quantitative binary/startup/RSS gates.
- Rename the GitHub repository and origin only at final cutover.
- Publish `antex-v0.0.1` only after every Phase 10 gate succeeds.

Portable Codex JSONL sessions and plain prompt stashes now migrate explicitly
without provider credentials or hidden instructions. The runtime/CLI validation
passed 55 tests on deimos. Rich legacy stash state is reported and skipped.
The module-size inventory reports no production module over 800 lines. The full
TUI suite passes 782 tests with one existing skip, and workspace release Clippy
passes with `-D warnings` on deimos.

## Validation on deimos

Remote checkout: `/home/antonmoss/antex-work/validation-host`.
Tools: `/home/antonmoss/antex-tools`.

From the local repository root:

```sh
rsync -a --delete --exclude target/ \
  antex-rs/ \
  deimos.vla.yp-c.yandex.net:/home/antonmoss/antex-work/validation-host/antex-rs/

python3 scripts/antex-remote.py \
  --host deimos.vla.yp-c.yandex.net \
  --checkout /home/antonmoss/antex-work/validation-host \
  --tools /home/antonmoss/antex-tools \
  env ANTEX_BWRAP=/home/antonmoss/antex-tools/bin/bwrap just test
```

Do not reset or reuse the older dirty checkout at
`/home/antonmoss/antex-work/codex`. The user-owned untracked `research/`
directory is not part of Antex commits.
