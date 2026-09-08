use antex_core::ContextHook;
use antex_core::ContextKind;
use antex_core::Message;
use antex_runtime::PermissionProfile;
use antex_runtime::ProjectContext;
use antex_runtime::WorkspaceFiles;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn environment_identifies_the_canonical_workspace_for_relative_paths() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let context = ProjectContext::load(home.path(), workspace.path()).unwrap();
    let messages = context.prepare(&[]).await.unwrap().messages;
    let expected = format!(
        "Execution environment\nCurrent working directory: {}\nResolve relative file paths against this directory.",
        serde_json::to_string(&workspace.path().canonicalize().unwrap()).unwrap()
    );
    let environment = messages.iter().find_map(|message| match message {
        Message::Context(fragment) if fragment.text().starts_with("Execution environment\n") => {
            Some(fragment.text())
        }
        _ => None,
    });
    assert_eq!(environment, Some(expected.as_str()));
}

#[tokio::test]
async fn nested_instructions_are_ordered_bounded_and_leave_history_unchanged() {
    let home = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    let workspace = root.path().join("nested");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(root.path().join("AGENTS.md"), "outer").unwrap();
    let nested = "я".repeat(6000);
    std::fs::write(workspace.join("AGENTS.md"), &nested).unwrap();
    let context = ProjectContext::load(home.path(), &workspace).unwrap();
    let history = vec![Message::User("request".into())];
    let prepared = context.prepare(&history).await.unwrap().messages;
    let projects: Vec<_> = prepared
        .iter()
        .filter_map(|message| match message {
            Message::Context(fragment) if fragment.kind() == ContextKind::Project => Some(fragment),
            _ => None,
        })
        .collect();
    assert!(
        projects
            .iter()
            .all(|fragment| fragment.text().len() <= antex_core::MAX_TEXT_BYTES)
    );
    assert_eq!(projects[0].text().split_once('\n').unwrap().1, "outer");
    assert_eq!(
        projects[1..]
            .iter()
            .map(|fragment| fragment.text().split_once('\n').unwrap().1)
            .collect::<String>(),
        nested
    );
    assert_eq!(&prepared[prepared.len() - 1..], history.as_slice());
    assert_eq!(context.prepare(&history).await.unwrap().messages, prepared);
}

#[tokio::test]
async fn global_skills_are_advertised_and_readable_but_not_writable() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let skill = home.path().join("skills/example");
    std::fs::create_dir_all(&skill).unwrap();
    let text = "---\nname: example\ndescription: >\n  Useful for testing\n  bounded context.\n---\nInstructions\n";
    std::fs::write(skill.join("SKILL.md"), text).unwrap();
    let context = ProjectContext::load(home.path(), workspace.path()).unwrap();
    let prepared = context.prepare(&[]).await.unwrap().messages;
    assert!(prepared.iter().any(|message|matches!(message,Message::Context(fragment) if fragment.kind()==ContextKind::Skill && fragment.text().contains("Useful for testing bounded context."))));
    let files = WorkspaceFiles::new(workspace.path(), PermissionProfile::Workspace)
        .unwrap()
        .with_read_roots(&context.read_roots)
        .unwrap();
    let path = skill.join("SKILL.md");
    assert_eq!(files.read(path.to_str().unwrap()).unwrap(), text.as_bytes());
    assert!(files.write(path.to_str().unwrap(), b"changed").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes());
}

#[cfg(unix)]
#[test]
fn symlinked_instruction_files_cannot_disclose_outside_data() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "private").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret"), home.path().join("AGENTS.md"))
        .unwrap();
    assert!(ProjectContext::load(home.path(), workspace.path()).is_err());
}
