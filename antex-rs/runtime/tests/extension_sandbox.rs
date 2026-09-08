#![cfg(target_os = "linux")]

use std::ffi::OsString;
use std::process::Stdio;

use antex_runtime::ExtensionSandbox;
use antex_runtime::ExtensionWorkspace;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

fn sandbox(home: &std::path::Path, workspace: &std::path::Path) -> ExtensionSandbox {
    let bubblewrap = std::env::var_os("ANTEX_BWRAP")
        .map(Into::into)
        .unwrap_or_else(|| "/usr/bin/bwrap".into());
    ExtensionSandbox::new(home, workspace, bubblewrap, &[]).unwrap()
}

#[tokio::test]
async fn protocol_stdin_remains_available_while_workspace_is_hidden() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let secret = workspace.path().join("secret");
    std::fs::write(&secret, "private").unwrap();
    let program = workspace.path().join(".antex/extensions/fixture/run");
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nread value; test ! -e {}; printf '%s' \"$value\"\n",
            shlex::try_quote(secret.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut command = sandbox(home.path(), workspace.path())
        .command(&program, &[], ExtensionWorkspace::Hidden, false)
        .await
        .unwrap();
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"protocol\n")
        .await
        .unwrap();
    let mut output = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .await
        .unwrap();
    assert!(child.wait().await.unwrap().success());
    assert_eq!(output, "protocol");
}

#[tokio::test]
async fn workspace_and_network_grants_change_only_the_declared_capability() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let target = workspace.path().join("written");
    let runtime = sandbox(home.path(), workspace.path());
    for (access, succeeds) in [
        (ExtensionWorkspace::ReadOnly, false),
        (ExtensionWorkspace::ReadWrite, true),
    ] {
        let script = format!(
            "printf data > {}",
            shlex::try_quote(target.to_str().unwrap()).unwrap()
        );
        let status = runtime
            .command(
                std::path::Path::new("/bin/sh"),
                &[OsString::from("-c"), OsString::from(script)],
                access,
                false,
            )
            .await
            .unwrap()
            .status()
            .await
            .unwrap();
        assert_eq!(status.success(), succeeds);
    }
    for (network, succeeds) in [(false, false), (true, true)] {
        let status = runtime
            .command(
                std::path::Path::new("/usr/bin/python3"),
                &[
                    OsString::from("-c"),
                    OsString::from("import socket; socket.socket()"),
                ],
                ExtensionWorkspace::Hidden,
                network,
            )
            .await
            .unwrap()
            .status()
            .await
            .unwrap();
        assert_eq!(status.success(), succeeds);
    }
}

#[tokio::test]
async fn workflow_shell_actions_share_bounded_sandbox_execution() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let result = sandbox(home.path(), workspace.path())
        .run_shell(
            "printf output; printf error >&2; printf data > written",
            ExtensionWorkspace::ReadWrite,
            /*network*/ false,
            std::time::Duration::from_secs(/*secs*/ 2),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        antex_runtime::ShellResult {
            exit_code: Some(0),
            stdout: "output".into(),
            stderr: "error".into(),
        }
    );
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("written")).unwrap(),
        "data"
    );
}
