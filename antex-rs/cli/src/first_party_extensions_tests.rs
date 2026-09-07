use std::fs;

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn embedded_extension_install_is_bounded_idempotent_and_non_destructive() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        names().collect::<Vec<_>>(),
        vec!["agents", "diagnostics", "tmux-log", "workflows"]
    );
    install(home.path(), "tmux-log").unwrap();
    let directory = home.path().join("extensions/tmux-log");
    assert_eq!(
        fs::read(directory.join("antex_ext_tmux_log.py")).unwrap(),
        TMUX_LOG_PROGRAM
    );
    install(home.path(), "tmux-log").unwrap();
    fs::write(directory.join("extension.json"), b"user change").unwrap();
    assert!(install(home.path(), "tmux-log").is_err());
    assert_eq!(
        fs::read(directory.join("extension.json")).unwrap(),
        b"user change"
    );
    assert!(install(home.path(), "unknown").is_err());
}
