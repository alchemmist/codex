# Antex tmux command log extension

Install `antex_ext_tmux_log.py` and this definition under
`~/.antex/extensions/tmux-log/`:

```json
{
  "name": "tmux-log",
  "program": "antex_ext_tmux_log.py",
  "capabilities": ["persist", "ui"]
}
```

Run `/tmux-command-log` inside tmux to toggle a private, bounded log window for
shell commands and their final output. Antex works normally when the extension
is absent or disabled.
