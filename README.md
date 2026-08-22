# Codex fork

Personal fork of [OpenAI Codex](https://github.com/openai/codex), kept in sync with upstream.

## What's included

- The TUI uses the terminal palette and updates the composer, conversation history, plans, and diffs immediately when the terminal theme changes.
- `Ctrl+S` stashes the current prompt draft and restores it on the next press.
- `/subagents <prompt>` explicitly enables subagents for one request; `/subagents` arms them for the next prompt.
- `/statusline` can show the number of active subagents, while `/agents` opens an overview of their work.
- `/cd <path>` changes the current session's working directory without restarting Codex.
- `/todo` shows the complete plan, while a compact adaptive list of active items remains visible at the bottom of the TUI.
- `/workflow` runs configurable Python workflows with persistent state and parallel-agent support.
- `/tmux-command-log` creates a separate tmux window containing Codex commands and their output.
- `/context` summarizes the model-visible context, while `/system-prompt` opens the complete latest logical model request in Neovim inside a new tmux window.
- `/dump` exports the full conversation to a responsive HTML file styled like [alchemmist.xyz](https://alchemmist.xyz), with tool activity collapsed between messages.
- The composer and submitted user messages share a cyan vertical rail, making prompts easy to find throughout the conversation.
- Prompts interrupted before work begins return to the editor; later interruptions are shown without a noisy error message.
- Fixes include tmux pane resize redraws, focus-related flickering, and a stable `Working` animation.
- The root `Makefile` builds and installs the local binary with one command.

## Build and install

```shell
make install
```

Upstream documentation: [developers.openai.com/codex](https://developers.openai.com/codex).
