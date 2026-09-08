# macOS shell startup regression

The installed build returned `exit: None` for `pwd`, directory listing and curl.
A native subprocess harness reproduced it without a model or Antex's process
supervision: sandbox-exec launched /bin/sh, which aborted with SIGABRT before
executing the script. The crash stack ended in dyld's ignition/boot initialization.

The hand-written profile had omitted the fork's rule allowing reads of the root
directory itself. The original rule is in
`alchemmist-v0.0.14:codex-rs/sandboxing/src/seatbelt_read_only_platform_defaults.sbpl`:

```scheme
(allow file-read* file-test-existence (literal "/"))
```

Restoring that literal rule fixed both home and temporary-directory launches.
It does not grant recursive filesystem access. Three native tests exercise the
actual shared base policy, including denial of reads outside the workspace.
The Rust shell now reports Unix termination signals explicitly; a public Shell
test reproduced the former `exit_code: None` result before the change.

The user also had `danger-full-access` and `approval_policy = "never"` in their
existing fork configuration. Antex had only received the TUI preferences and
still used its default workspace/no-network profile. The user's Antex config now
selects `permissions = "full"`. The default restricted profiles remain restricted;
the shell description tells the model which session policy is active.
Runtime context now supplies the canonical cwd for relative file paths.

The previous macOS acceptance claim was too broad: its journal contains three
successful file results and a failed shell result, despite ANTEX_MAC_OK from the
model. That response is not valid evidence of a successful tool chain. Subsequent
acceptance must inspect every tool outcome and the concrete outputs.

Verification after the fix:

- Three native policy tests passed, including the unrelated-file denial.
- 861 Rust tests across runtime, TUI and CLI passed; the subsequent focused
  frontend run passed six tests. Final local formatting and remote Clippy fix
  completed without diagnostics.
- Native session cab15a9b-3af9-419d-b8c4-916e60181bf3 ran pwd, read the current
  Cargo workspace and fetched https://alchemmist.xyz/. All three recorded tool
  outcomes were success. Explicit journal assertions checked cwd, Cargo content
  and the HTML response with exit code zero.
- Session 189b0cb7-0a75-4ae2-a76a-6d6f19f1a052 additionally verified pwd with an
  explicit workspace sandbox, independently of the user's restored full profile.
- The fork's Working row was restored above the composer with shimmer/activity,
  elapsed time and a working Escape interrupt. Native tmux showed the running
  row and then Interrupted after Escape.
- The signed build 0.0.0+d6ec69910f was installed atomically at
  /Users/antonmoss/.local/bin/antex and verified through interactive zsh.
  SHA-256: 8e3997534f56b2265ec2c29af4ff2aa90e1d21ea76d96a0d0362a2fbcb9d2ef2.

The requested old codex-rs directory no longer contains Cargo.toml. Current
sources are under /Users/antonmoss/code/codex/antex-rs; no compatibility symlink
or silent path substitution was added.
