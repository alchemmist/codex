use std::fs;

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn tmux_log_uses_a_private_bounded_file_and_removes_it_on_drop() {
    let home = tempfile::tempdir().unwrap();
    let program = home.path().join("tmux");
    fs::write(&program, "#!/bin/sh\nprintf '%%7\\n'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut log = TmuxLog::start_with_program(home.path(), Uuid::nil(), &program).unwrap();
    let path = log.path.clone();
    log.append("$ pwd").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "$ pwd\n");
    assert!(log.append(&"x".repeat(MAX_LOG_BYTES)).is_err());
    drop(log);
    assert!(!path.exists());
}

#[test]
fn shell_quote_preserves_single_quotes() {
    assert_eq!(shell_quote("a'b"), "'a'\\''b'");
}
