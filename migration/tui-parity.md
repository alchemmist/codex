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
