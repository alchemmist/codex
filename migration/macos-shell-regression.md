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
