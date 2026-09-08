# Antex explicit agents extension

Install it with `antex extensions install agents`.

- `/subagents <prompt>` runs one ephemeral agent.
- `/agents ["prompt one", "prompt two"]` runs up to eight independent agents.
- `/agents` shows the bounded history of explicitly requested runs.

Only these user commands can produce agent actions. Ordinary prompts and model
tools cannot start subagents.
