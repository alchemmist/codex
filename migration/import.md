# Codex import during extraction

The development CLI adds `antex migrate codex --dry-run` and
`antex migrate codex`. Compiled validation is pending in GitHub Actions; this is
not a released Antex migration tool.

The source is `CODEX_HOME`, or `~/.codex` when unset. The destination is
`ANTEX_HOME` (an existing directory), or `~/.antex`. Source and destination must
not overlap. Dry-run reads the source and emits a deterministic JSON action
report without creating the default destination or writing files.

The import copies regular files from `sessions/`, `prompt-stashes/`, and `skills/`.
It imports selected model/context settings, approval and sandbox settings, MCP
connection definitions, skill selectors, and the terminal theme from config.toml.
Unsupported configuration keys are listed as `skip-config` actions. Skill paths
inside the source are translated to the destination; existing external skill
references are resolved to their external paths.

OAuth auth.json, keyring data, databases, caches, and other source-root entries
are not imported. MCP environment values and HTTP headers remain part of their
connection definitions, but configuration values are never included in reports.
Source symlinks are skipped and reported; destination symlinks are rejected.

Existing destination files are skipped, including config.toml. Importing again
therefore preserves changes already made in Antex. Each new file is published
without overwriting an existing file, after writing and syncing a temporary file.
If copying fails, already imported files remain; retrying preserves them.

Limits are 10,000 visited entries, 1 MiB of configuration, and 64 MiB per copied
file. Exceeding a limit fails explicitly. No command executes imported MCP servers
or reads provider credentials. The imported sessions still use the legacy format
until the runtime/session-store extraction is completed.
