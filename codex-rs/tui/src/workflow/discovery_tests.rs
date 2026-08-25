use pretty_assertions::assert_eq;
use tokio::process::Command;

use super::*;
use crate::workflow::python_host::python_program;

#[tokio::test]
async fn project_workflow_overrides_user_and_builtin_workflows() {
    if Command::new(python_program())
        .arg("--version")
        .output()
        .await
        .is_err()
    {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let codex_home = temp.path().join("home");
    let cwd = temp.path().join("project");
    tokio::fs::create_dir_all(codex_home.join("workflows"))
        .await
        .expect("create user workflow dir");
    tokio::fs::create_dir_all(cwd.join(".codex/workflows"))
        .await
        .expect("create project workflow dir");
    tokio::fs::write(
        codex_home.join("workflows/user.py"),
        "WORKFLOW = {'id':'same','title':'User'}\ndef run(ctx): pass\n",
    )
    .await
    .expect("write user workflow");
    tokio::fs::write(
        cwd.join(".codex/workflows/project.py"),
        "WORKFLOW = {'id':'same','title':'Project'}\ndef run(ctx): pass\n",
    )
    .await
    .expect("write project workflow");

    let (definitions, diagnostics) = discover_workflows(&codex_home, &cwd).await;

    assert_eq!(diagnostics, Vec::<String>::new());
    assert!(definitions.iter().any(|definition| {
        definition.manifest.id == "same"
            && definition.manifest.title == "Project"
            && definition.source == "project"
    }));
    assert!(
        definitions
            .iter()
            .any(|definition| definition.manifest.id == BUILTIN_RUFF_WORKFLOW_ID)
    );
    assert!(definitions.iter().any(|definition| {
        definition.manifest.id == BUILTIN_GITHUB_BOT_PR_WORKFLOW_ID
            && definition.manifest.fields.iter().any(|field| {
                field.id == "action" && field.default == Some(serde_json::json!("merge"))
            })
    }));
    assert!(
        codex_home
            .join("workflow-cache/github-bot-pr-maintenance-v1.py")
            .is_file()
    );
}
