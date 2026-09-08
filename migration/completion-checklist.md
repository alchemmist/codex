# Antex completion audit

The phase gates in PLAN.md remain authoritative. An unchecked gate is not a
claim that its implementation is absent; it means acceptance evidence is still
incomplete. Starting checkpoint: fbcced88c7246b35774b3517117d6d7874a1a11c.

- [ ] Phase 0: reconcile baseline fixtures and the feature matrix with the final path.
- [ ] Phase 1: verify identity and non-destructive migration acceptance.
- [ ] Phase 2: verify the public kernel tests and dependency boundary.
- [ ] Phase 3: complete provider contracts, live subscription use and identity probe.
- [ ] Phase 4: verify tools, sandbox, sessions, compaction and context acceptance.
- [ ] Phase 5: verify direct TUI fixtures and supported-platform terminal smoke.
- [ ] Phase 6: verify extension conformance, limits and failure isolation.
- [ ] Phase 7: finish workflow control/discovery/recovery, packaged babysitter,
      MCP OAuth and extension acceptance.
- [ ] Phase 8: verify forbidden dependencies and quantitative budgets.
- [ ] Phase 9: finish repository rename and verify ancestry and historical tags.
- [ ] Phase 10: pass platform preflight, installer checks and publish 0.0.1.

## Validation constraints

Rust execution uses scripts/antex-remote.py on deimos. The full workspace suite
requires explicit user approval under AGENTS.md. Focused crate tests do not.
After focused tests pass, run local just fmt and remote just fix; do not rerun
tests after that final pass for the same slice.

The user-owned research/ directory and installed Codex fallback are outside the
change scope. Live OAuth approval and measured macOS execution are outstanding;
neither mock tests nor Linux cross-compilation satisfy those gates.
