use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn image_commands_normalize_selected_files_without_contacting_the_provider() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config::default(),
        /*bubblewrap*/ None,
        provider,
    )
    .unwrap();
    let pixels = vec![255, 0, 0, 255, 0, 255, 0, 128];
    let expected = antex_runtime::ImageAttachment::from_rgba(
        /*width*/ 2,
        /*height*/ 1,
        pixels.clone(),
    )
    .unwrap()
    .content;
    assert_eq!(
        session
            .prepare_image(antex_tui::ImageSource::Rgba {
                width: 2,
                height: 1,
                bytes: pixels
            })
            .await
            .unwrap(),
        expected
    );
    let antex_core::Content::Image { data, .. } = &expected else {
        panic!("expected normalized image");
    };
    std::fs::write(workspace.path().join("my image.png"), data).unwrap();
    let CommandEffect::Image(actual) = session.command("/image 'my image.png'").await.unwrap()
    else {
        panic!("expected attachment");
    };
    assert_eq!(actual, expected);
    assert!(session.history().is_empty());
}

#[tokio::test]
async fn help_page_covers_the_direct_frontend_commands() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config::default(),
        /*bubblewrap*/ None,
        provider,
    )
    .unwrap();
    let CommandEffect::Page(page) = session.command("/help").await.unwrap() else {
        panic!("expected help page");
    };
    insta::assert_snapshot!(page.body);
}

#[test]
fn agent_events_map_to_bounded_extension_lifecycle_events() {
    let call = antex_core::ToolCall {
        id: "call-1".into(),
        name: "shell".into(),
        arguments: serde_json::json!({"command":"pwd"}),
    };
    let started = extension_event(&AgentEvent::ToolStarted(call)).unwrap();
    assert_eq!(started.name, "toolStarted");
    assert_eq!(started.data["name"], "shell");

    let completed = extension_event(&AgentEvent::MessageCommitted(Message::Tool(
        antex_core::ToolOutput::new(
            "call-1".into(),
            antex_core::ToolOutcome::Success,
            "done".into(),
        ),
    )))
    .unwrap();
    assert_eq!(completed.name, "toolCompleted");
    assert_eq!(completed.data["text"], "done");
    assert!(extension_event(&AgentEvent::TextDelta("ignored".into())).is_none());
}

#[tokio::test]
async fn explicit_extension_inspection_uses_the_current_in_process_context() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config::default(),
        /*bubblewrap*/ None,
        provider,
    )
    .unwrap();
    let effect = session
        .apply_extension_output(
            "diagnostics",
            antex_extension_protocol::Output {
                actions: vec![antex_extension_protocol::Action::Inspect {
                    id: "system".into(),
                    target: antex_extension_protocol::Inspection::SystemPrompt,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let CommandEffect::Notice(text) = effect else {
        panic!("expected inspection output");
    };
    assert!(text.starts_with("Antex\nYou are Antex"));
    assert_eq!(
        session.conversation.extension_states().unwrap()["diagnostics"]["actionResult"]["id"],
        "system"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn personal_workflows_are_readable_without_exposing_adjacent_credentials() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    crate::first_party_extensions::install(home.path(), "workflows").unwrap();
    let workflows = home.path().join("workflows");
    std::fs::create_dir(&workflows).unwrap();
    std::fs::write(home.path().join("auth.json"), "private").unwrap();
    std::fs::write(
        workflows.join("check.py"),
        "from pathlib import Path\nWORKFLOW={'id':'check'}\ndef run(ctx):\n try:\n  Path(__file__).parent.parent.joinpath('auth.json').read_text()\n except OSError:\n  return {'private':True}\n raise RuntimeError('credentials exposed')\n",
    ).unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config::default(),
        std::env::var_os("ANTEX_BWRAP").map(Into::into),
        provider,
    )
    .unwrap();
    let CommandEffect::Notice(text) = session.command("/workflow check").await.unwrap() else {
        panic!("expected workflow result");
    };
    assert_eq!(text, "{\"private\": true}");
    assert_eq!(
        session.conversation.extension_states().unwrap()["workflows"]["phase"],
        "completed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn installed_python_workflow_branches_through_the_in_process_action_loop() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    crate::first_party_extensions::install(home.path(), "workflows").unwrap();
    let workflows = workspace.path().join(".antex/workflows");
    std::fs::create_dir_all(&workflows).unwrap();
    std::fs::write(
        workflows.join("check.py"),
        "WORKFLOW={'id':'check','title':'Check'}\ndef run(ctx):\n result=ctx.shell(['/bin/sh','-c','printf ok > workflow-result'])\n return {'exit':result['exitCode']}\n",
    )
    .unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config {
            trust_project_extensions: true,
            ..Config::default()
        },
        std::env::var_os("ANTEX_BWRAP").map(Into::into),
        provider,
    )
    .unwrap();
    let CommandEffect::Notice(text) = session.command("/workflow check").await.unwrap() else {
        panic!("expected workflow result");
    };
    assert_eq!(text, "{\"exit\": 0}");
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("workflow-result")).unwrap(),
        "ok"
    );
    assert_eq!(
        session.conversation.extension_states().unwrap()["workflows"]["workflow"],
        "check"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn installed_diagnostics_reads_context_and_exports_without_overwriting() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    crate::first_party_extensions::install(home.path(), "diagnostics").unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config::default(),
        std::env::var_os("ANTEX_BWRAP").map(Into::into),
        provider,
    )
    .unwrap();
    let CommandEffect::Page(page) = session.command("/system-prompt").await.unwrap() else {
        panic!("expected system prompt panel");
    };
    assert_eq!(page.title, "System prompt");
    assert!(page.body.starts_with("Antex\nYou are Antex"));
    session
        .record(&AgentEvent::MessageCommitted(Message::User(
            "request".into(),
        )))
        .await
        .unwrap();
    let CommandEffect::Notice(notice) = session.command("/dump report.md").await.unwrap() else {
        panic!("expected export notice");
    };
    assert_eq!(notice, "Exported transcript to report.md");
    let path = workspace.path().join("report.md");
    assert!(std::fs::read_to_string(&path).unwrap().contains("request"));
    assert!(session.command("/dump report.md").await.is_err());
}
