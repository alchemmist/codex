# Antex

The Rust workspace is `antex-rs/`. Antex is the only product binary and uses
`~/.antex`; the separately installed `codex 0.0.14` and `~/.codex` are fallback
state and remain untouched.

## Change loop

1. Keep the seven-crate dependency direction documented in `PLAN.md`.
2. Prefer extraction and deletion. Add a seam only when two real adapters need it.
3. Keep behavioral commits below 500 changed lines and non-mechanical commits
   below 800 lines.
4. Test the changed crates through their public interface.
5. Run Rust builds, tests and Clippy only on `deimos.vla.yp-c.yandex.net` through
   `scripts/antex-remote.py`.
6. After tests pass, run `just fmt` locally, then `just fix` on deimos. Tests are
   not rerun after the final fix/format pass.

The full suite requires explicit user approval. Focused crate tests do not.

## Rust

- Crate names use the `antex-` prefix.
- Inline `format!` arguments and collapse nested `if` expressions.
- Prefer method references to redundant closures.
- Prefer exhaustive `match` expressions.
- New traits document their role and use RPITIT with explicit `Send` futures;
  dynamic tool interfaces may return boxed futures.
- Prefer named enums or methods over opaque boolean and `Option` arguments.
- Tests compare complete values with `pretty_assertions::assert_eq`.
- New test modules live in sibling `*_tests.rs` files.
- User-visible TUI changes include reviewed `insta` snapshots.
- Production modules stay below 800 lines and preferably below 500.

## Product constraints

- `antex-core` remains provider-neutral and has no filesystem, terminal, HTTP,
  OAuth, Git, session or extension transport dependency.
- OpenAI/Codex transport identity remains private to `antex-provider-openai`.
- Context additions are bounded, append-only and represented by core context
  fragment types.
- Extensions are out-of-process, capability-sandboxed and receive no provider
  credentials.
- Supported release targets are Apple Silicon macOS and x86_64 GNU/Linux.
- Release tags use `antex-v<version>` and never derive from a Codex version.
