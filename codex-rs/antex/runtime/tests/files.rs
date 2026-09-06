use antex_runtime::PermissionProfile;
use antex_runtime::WorkspaceFiles;
use pretty_assertions::assert_eq;

#[test]
fn edits_preserve_crlf_unicode_and_file_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let files = WorkspaceFiles::new(directory.path(), PermissionProfile::Workspace).unwrap();
    files
        .write("file", "первая\r\nвторая\r\n".as_bytes())
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            directory.path().join("file"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    files.edit("file", "вторая", "новая").unwrap();
    assert_eq!(
        files.read("file").unwrap(),
        "первая\r\nновая\r\n".as_bytes()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(directory.path().join("file"))
                .unwrap()
                .mode()
                & 0o777,
            0o755
        );
    }
}

#[test]
fn ambiguous_edits_and_read_only_writes_leave_files_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let files = WorkspaceFiles::new(directory.path(), PermissionProfile::Workspace).unwrap();
    files.write("file", b"same same").unwrap();
    assert!(files.edit("file", "same", "different").is_err());
    let readonly = WorkspaceFiles::new(directory.path(), PermissionProfile::ReadOnly).unwrap();
    assert!(readonly.write("file", b"changed").is_err());
    assert!(readonly.edit("file", "same same", "changed").is_err());
    assert_eq!(files.read("file").unwrap(), b"same same");
}

#[test]
fn parent_traversal_cannot_escape_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("workspace");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(root.path().join("secret"), b"untouched").unwrap();
    let files = WorkspaceFiles::new(&directory, PermissionProfile::Workspace).unwrap();
    assert!(files.read("../secret").is_err());
    assert!(files.write("../secret", b"changed").is_err());
    assert_eq!(
        std::fs::read(root.path().join("secret")).unwrap(),
        b"untouched"
    );
}

#[cfg(unix)]
#[test]
fn symlink_escape_and_hardlink_writes_do_not_modify_outside_files() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("workspace");
    std::fs::create_dir(&directory).unwrap();
    let secret = root.path().join("secret");
    std::fs::write(&secret, b"untouched").unwrap();
    std::os::unix::fs::symlink(root.path(), directory.join("outside")).unwrap();
    std::fs::hard_link(&secret, directory.join("hardlink")).unwrap();
    let files = WorkspaceFiles::new(&directory, PermissionProfile::Workspace).unwrap();
    assert!(files.read("outside/secret").is_err());
    assert!(files.write("outside/secret", b"changed").is_err());
    files.write("hardlink", b"local").unwrap();
    assert_eq!(std::fs::read(&secret).unwrap(), b"untouched");
    assert_eq!(files.read("hardlink").unwrap(), b"local");
}
