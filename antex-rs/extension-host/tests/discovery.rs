use std::fs;

use antex_extension_host::DiscoveryError;
use antex_extension_host::discover;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

#[cfg(unix)]
fn extension(root: &std::path::Path, directory: &str, name: &str) {
    use std::os::unix::fs::PermissionsExt;

    let root = root.join(directory);
    fs::create_dir_all(&root).unwrap();
    let program = root.join("run");
    fs::write(&program, "#!/bin/sh\n").unwrap();
    let mut permissions = program.metadata().unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&program, permissions).unwrap();
    fs::write(
        root.join("extension.json"),
        format!(r#"{{"name":"{name}","program":"run"}}"#),
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn project_extensions_require_trust_and_results_are_stable() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    extension(&home.path().join("extensions"), "global", "z-global");
    extension(
        &workspace.path().join(".antex/extensions"),
        "project",
        "a-project",
    );
    assert_eq!(
        discover(home.path(), workspace.path(), false)
            .unwrap()
            .len(),
        1
    );
    let discovered = discover(home.path(), workspace.path(), true).unwrap();
    assert_eq!(
        discovered
            .iter()
            .map(|extension| (&extension.name, extension.project))
            .collect::<Vec<_>>(),
        vec![
            (&"a-project".to_string(), true),
            (&"z-global".to_string(), false)
        ]
    );
}

#[cfg(unix)]
#[test]
fn duplicate_names_fail_closed() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    extension(&home.path().join("extensions"), "first", "same");
    extension(&home.path().join("extensions"), "second", "same");
    assert!(matches!(
        discover(home.path(), workspace.path(), true),
        Err(DiscoveryError::Duplicate(name)) if name == "same"
    ));
}
