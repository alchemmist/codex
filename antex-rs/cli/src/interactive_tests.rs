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
