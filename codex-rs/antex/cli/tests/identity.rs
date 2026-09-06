use std::process::Command;

#[test]
fn version_is_independent_and_does_not_initialize_data_directories() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("not-created");
    let output = Command::new(env!("CARGO_BIN_EXE_antex"))
        .arg("--version")
        .env("ANTEX_HOME", &home)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("antex 0.0.0+")
    );
    assert!(!home.exists());
}

#[test]
fn legacy_home_override_is_rejected_without_writing_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join(".codex");
    std::fs::create_dir(&legacy).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_antex"))
        .arg("accounts")
        .env("HOME", directory.path())
        .env("ANTEX_HOME", &legacy)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_dir(&legacy).unwrap().count(), 0);
}

#[test]
fn failed_provider_turn_is_persisted_and_can_be_resumed_without_credentials() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let run = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_antex"))
            .arg("--cd")
            .arg(workspace.path())
            .args(arguments)
            .env("HOME", home.path())
            .env("ANTEX_HOME", home.path())
            .output()
            .unwrap()
    };
    assert!(!run(&["exec", "--model", "fake", "first"]).status.success());
    let listing = run(&["sessions"]);
    assert!(listing.status.success());
    let id = String::from_utf8(listing.stdout).unwrap();
    assert!(
        !run(&["exec", "--model", "fake", "--resume", id.trim(), "second"])
            .status
            .success()
    );
    let store = antex_runtime::SessionStore::new(home.path(), workspace.path()).unwrap();
    let history = store
        .open(id.trim().parse().unwrap())
        .unwrap()
        .active_path()
        .unwrap();
    pretty_assertions::assert_eq!(
        history
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>(),
        vec![
            antex_core::Message::User("first".into()),
            antex_core::Message::User("second".into())
        ]
    );
}
