# TUI restoration

Visual authority: `alchemmist-v0.0.14`, as explicitly requested by the maintainer.
Functional smoke results from the migration do not establish visual parity.

Restore from these original presentation implementations:

- `tui/src/history_cell/messages.rs`: user rail, spacing and assistant bullets;
- `tui/src/bottom_pane/chat_composer.rs`: three-column text inset, full-height
  rail, top/bottom padding and footer;
- `tui/src/history_cell/startup_panel.rs`: framed startup card, mascot alignment
  and narrow fallback;
- `tui/src/line_truncation.rs`: grapheme-safe styled truncation;
- `tui/src/style.rs`: terminal-native message background and light-safe accents.

The maintainer's existing TUI preferences are ANSI syntax colors, Vim enabled
starting in insert mode, and `status_line = ["current-dir", "model"]`.
The Codex settings remain untouched. Antex must honor these same supported keys.

Two differential tests reproduced the regression before changes: the user rail
had become a separate chevron line, and the composer had lost its rail/insets.
They now use the original rendering contract, not the migration's replacement
snapshots. Complete visual and behavioral parity remains an acceptance gate;
do not infer it from successful login, a model response or a passing build.

The first native visual pass found an additional migration regression: resize
left terminal-wrapped historical rows instead of rebuilding from message source.
The restored reflow path follows the fork's reset/replay lifecycle, 75 ms resize
debounce and 1000-row fallback cap. It retains startup rendering, rebuilds user
rails at the new width and also replays when the syntax theme revision changes.
The focused resize fixture checks rail continuity and repeated-replay stability.
The final TUI run passed 786 tests with one existing skip and one reported leaky
test; the leaky-test observation is not a completed lifecycle acceptance gate.

Native confirmation used the cross-built `0.0.0+9de274b516` on the maintainer's
Mac. The conversation view was checked at 100x30, resized to 32x16 and widened
again; the card stopped wrapping incorrectly and every wrapped user line kept
its rail. A light tmux pane and `/theme ansi` replay preserved readable native
colors and did not duplicate the message. The test sessions were closed.
The signed binary is installed at `/Users/antonmoss/.local/bin/antex`; interactive
zsh resolves and runs it. Its SHA-256 is
`0d53769f35ed392c097908d31ec1b3a768cf3af9d6a1aaa18fdb05790a5a314b`.
