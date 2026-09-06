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
