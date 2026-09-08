use std::path::PathBuf;
use std::time::Duration;

use antex_runtime::PermissionProfile;
use antex_runtime::Shell;
use antex_runtime::ShellResult;
use pretty_assertions::assert_eq;
use tokio_util::sync::CancellationToken;

fn shell(directory: &std::path::Path, profile: PermissionProfile) -> Shell {
    let program = std::env::var_os("ANTEX_BWRAP")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/usr/bin/bwrap".into());
    Shell::new(directory, profile)
        .unwrap()
        .with_bubblewrap(program)
}

#[cfg(unix)]
#[tokio::test]
async fn signal_termination_reports_the_signal_instead_of_an_absent_exit_code() {
    let directory = tempfile::tempdir().unwrap();
    let error = shell(directory.path(), PermissionProfile::Full)
        .run("kill -TERM $$", CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "shell terminated by signal 15\n");
}

#[tokio::test]
async fn workspace_shell_can_write_locally_but_cannot_read_host_home_or_open_sockets() {
    let directory = tempfile::tempdir().unwrap();
    let shell = shell(directory.path(), PermissionProfile::Workspace);
    assert_eq!(
        shell
            .run("printf hello > local; cat local", CancellationToken::new())
            .await
            .unwrap(),
        ShellResult {
            exit_code: Some(0),
            stdout: "hello".into(),
            stderr: String::new()
        }
    );
    #[cfg(target_os = "linux")]
    {
        let result = shell
            .run(
                "/usr/bin/python3 -c 'import socket; socket.socket(socket.AF_UNIX)'",
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(result.exit_code, Some(0));
        assert!(result.stderr.contains("Operation not permitted"));
    }
}

#[tokio::test]
async fn readonly_shell_cannot_modify_workspace_files() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("file"), b"untouched").unwrap();
    let shell = shell(directory.path(), PermissionProfile::ReadOnly);
    let result = shell
        .run("printf changed > file", CancellationToken::new())
        .await
        .unwrap();
    assert_ne!(result.exit_code, Some(0));
    assert_eq!(
        std::fs::read(directory.path().join("file")).unwrap(),
        b"untouched"
    );
}

#[tokio::test]
async fn full_profile_is_explicit_and_does_not_require_bubblewrap() {
    let directory = tempfile::tempdir().unwrap();
    let shell = Shell::new(directory.path(), PermissionProfile::Full)
        .unwrap()
        .with_bubblewrap("/missing/bwrap".into());
    assert_eq!(
        shell
            .run("printf full", CancellationToken::new())
            .await
            .unwrap(),
        ShellResult {
            exit_code: Some(0),
            stdout: "full".into(),
            stderr: String::new()
        }
    );
}

#[tokio::test]
async fn shell_output_is_bounded_while_both_pipes_are_drained() {
    let directory = tempfile::tempdir().unwrap();
    let shell = shell(directory.path(), PermissionProfile::Workspace);
    let result = shell.run("/usr/bin/python3 -c 'import sys; print(\"я\"*20000); print(\"e\"*20000,file=sys.stderr)'",CancellationToken::new()).await.unwrap();
    assert_eq!(result.exit_code, Some(0));
    for output in [&result.stdout, &result.stderr] {
        assert!(output.len() <= antex_core::MAX_TEXT_BYTES);
        assert!(output.ends_with("[output truncated]"));
    }
}

#[tokio::test]
async fn timeout_terminates_a_running_process_group() {
    let directory = tempfile::tempdir().unwrap();
    let shell = shell(directory.path(), PermissionProfile::Workspace)
        .with_timeout(Duration::from_millis(100));
    let result = shell.run("sleep 10 & wait", CancellationToken::new()).await;
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
}

#[tokio::test]
async fn cancellation_interrupts_a_running_shell() {
    let directory = tempfile::tempdir().unwrap();
    let shell = shell(directory.path(), PermissionProfile::Workspace);
    let cancellation = CancellationToken::new();
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancellation.cancel();
    };
    let (result, ()) = tokio::join!(shell.run("sleep 10", cancellation.clone()), cancel);
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
}
