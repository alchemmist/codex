# Behavioral baseline

Baseline: `c6ca7fe8a7e6eb9bc7dad2904c92b3b824a36a82`, upstream `0.153.4`,
fork `0.0.14`. Each row maps one current README feature to its owner phase.
Disposition does not imply implementation or verification.

| README feature | Disposition | Owner phase |
| --- | --- | --- |
| Terminal palette and immediate theme updates | Keep | 5 |
| Startup cockpit, commit, tips, both animated ant skins | Keep with Antex identity | 5 |
| Persistent Ctrl+S prompt stash | Keep | 5 |
| Explicit /subagents and next-prompt arming | Extension | 7 |
| Active-agent status and /agents | Extension | 7 |
| Process-local fast mode and status | Keep | 5 |
| /cd without restarting | Keep | 4 |
| /todo and adaptive persistent plan | Extension | 7 |
| Python workflows and PR babysitter | Extension | 7 |
| tmux command logging | Extension | 7 |
| /context and /system-prompt | Extension | 7 |
| /dump HTML export | Extension | 7 |
| Cyan prompt rails | Keep | 5 |
| Vim modes, Russian aliases, clipboard, selections | Keep | 5 |
| Early interruption restores prompt; quiet later interruption | Keep | 5 |
| Explicit force-push approval | Keep in runtime permissions | 4 |
| tmux resize, focus redraw, stable Working animation | Keep | 5 |
| Local and downloaded macOS/Linux installation | Replace with Antex installers | 10 |

## Reproduction

`make migration-baseline` emits JSON without fetching dependencies or reading
credentials. Populate Cargo's cache through the normal build first if necessary.
`make migration-baseline BASELINE_ARGS='--binary /absolute/path/to/codex'` also
records size, SHA-256, and version command latency. It does not install anything.
`make test-migration` tests the dependency traversal independently of Cargo.

`make migration-smoke BASELINE_BINARY=/absolute/path/to/codex` runs the actual
binary with a local fake Responses server, dummy credentials, and temporary
config/workspace directories. It verifies an assistant response, a shell tool
round trip, the next model request's tool result, persisted JSONL output, session
resume, and image input. Use `SMOKE_ARGS='--scenario image'` to isolate a case.

`BASELINE_ARGS=--check` enables the static deletion/size-of-source gates. It is
expected to fail for the baseline. This is not the complete release preflight:
startup, memory, stripped binary size, live acceptance, and both target builds
still need their own evidence. Source counts are conservative upper bounds;
inline tests, comments, and target-specific source remain included.

## Existing fixture anchors

These are extraction anchors, not a claim that the required new black-box suite
is complete. Every migration phase must replace the relevant anchors with tests
at the new interface before deleting their implementation.

| Behavior | Existing anchor | Verification |
| --- | --- | --- |
| OAuth token loading and refresh | `codex-rs/login/tests/suite/auth_refresh.rs` | All 39 login integration tests passed on macOS |
| Assistant response, persistence | `scripts/antex-smoke.py` | Installed legacy binary passed on macOS |
| Shell execution and next model request | `scripts/antex-smoke.py` | Installed legacy binary passed on macOS |
| Session resume | `scripts/antex-smoke.py` | Installed legacy binary passed on macOS |
| Fork and compaction | `codex-rs/core/tests/suite/compact_resume_fork.rs` | Selected integration test passed on macOS |
| Image context | `scripts/antex-smoke.py`, `codex-rs/core/tests/suite/image_rollout.rs` | Both passed on macOS |
| Interruption | `codex-rs/core/tests/suite/abort_tasks.rs` | Selected integration test passed on macOS |
| Steering | `codex-rs/core/tests/suite/pending_input.rs` | Selected integration test passed on macOS |
| Model catalog refresh | `codex-rs/core/tests/suite/models_cache_ttl.rs` | Selected integration test passed on macOS |
| TUI width and theme | `codex-rs/tui/src/history_cell/tests.rs`, `codex-rs/tui/src/app/tests.rs` | Three selected snapshot/theme tests passed on macOS |

## Reproduction commands for retained integration fixtures

Run these from `codex-rs/`:

```sh
just test -p codex-login --test all
just test -p codex-core --test all -E 'test(compact_resume_and_fork_preserve_model_history_view) | test(interrupt_long_running_tool_emits_turn_aborted) | test(steer_interrupts_wait_agent_and_is_sent_in_follow_up_request) | test(copy_paste_local_image_persists_rollout_request_shape) | test(renews_cache_ttl_on_matching_models_etag)'
just test -p codex-tui --lib -E 'test(startup_panel_mascot_skins_and_narrow_fallback_snapshot) | test(transcript_reflow_restyles_existing_user_message_after_theme_change) | test(theme_change_schedules_source_backed_transcript_reflow)'
```

## External prerequisites

GitHub MCP found the remote annotated tag `alchemmist-v0.0.14` pointing at the
baseline commit, but `get_release_by_tag` returned 404. `list_releases` reported
`alchemmist-v0.0.13` as the newest published release. Thus the plan's assumption
that a published `0.0.14` fallback exists was not verified. The maintainer has
explicitly accepted the current local `0.0.14` as the fallback; publication is
not a migration prerequisite. The installed binary is retained without
modification; its fingerprint is in `baseline-macos.json`.

The current Mac has approximately 11–14 GiB available and an existing 133 GiB
Cargo target directory. The maintainer deferred clean platform builds and
quantitative measurements to Phase 10 GitHub Actions release preflight. A local
clean-build destination and Linux host are no longer migration prerequisites.
Existing target data is not removed.
The known `deimos.vla.yp-c.yandex.net` host was checked with noninteractive SSH;
the connection failed with `No route to host` before authentication.
The maintainer confirmed corporate VPN is currently disconnected. Remote builds
on deimos are authorized but deferred until VPN availability is announced;
GitHub Actions remains the active compiled-validation path.

The earlier `make build` process is no longer running and its completion result
was not recovered. No successful rebuilt release or build-duration measurement
is claimed. The maintainer subsequently prohibited local builds and tests that
trigger compilation; subsequent compiled validation belongs in GitHub Actions.

## Phase 0 work remaining

- [x] Create `antex` from current `main` and record the baseline commit.
- [x] Synchronize the phase checklist with PLAN.md.
- [x] Map every current README feature to one disposition and owner phase.
- [x] Add reproducible workspace, dependency, source, binary inventory.
- [ ] Phase 10: measure clean/warm builds, editable prompt startup, idle memory on both targets in GitHub Actions; deferred by the maintainer.
- [ ] Complete and pass all required black-box fixtures.
- [x] Retain the current local `0.0.14` fallback, explicitly accepted by the maintainer without a published release.

The deferred Phase 10 measurements do not block migration work. Fixture coverage
still needs to satisfy Phase 0; later phases retain their own completion gates.

## Phase 1 progress

- [x] State the independent Antex identity and migration status in README.
- [x] Record Codex ancestry in NOTICE while preserving existing attribution.
- [ ] Introduce the Antex composition root and isolated data home.
- [ ] Implement explicit, non-destructive Codex data import.
- [ ] Complete runtime/UI branding and product environment-variable changes.

Documentation reflects the current implementation; the existing install targets
still install the legacy runtime and must not overwrite the accepted fallback.

The first identity slice adds an `antex` binary target, independent development
version output, and a temporary entry adapter that isolates both config and SQLite
state from the fallback. CLI integration tests cover version output without state
creation, MCP config writes with conflicting legacy overrides, and rejection of
home aliases into `.codex`. These compiled tests await the `antex-migration`
GitHub Actions workflow; no local build or test compilation was run for this slice.

The first Linux workflow built the Antex binary and passed three identity tests.
The fourth failed because the test parsed a complete TOML document through
`Value::from_str`, which parses a value. It now uses `toml::from_str`. The same
correction is applied to the import implementation and its tests. A new workflow
run must validate the fix and import behavior before Phase 1 is complete.

The explicit import command and its pending CI coverage are described in
[`import.md`](import.md). The maintainer authorized `gh` as well as GitHub MCP;
workflow logs can now be read directly without browser automation.
