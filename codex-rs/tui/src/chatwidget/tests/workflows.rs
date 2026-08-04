use super::*;
use crate::workflow::WorkflowDefinition;
use crate::workflow::WorkflowUpdate;

#[tokio::test]
async fn workflow_picker_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let definitions = vec![WorkflowDefinition {
        manifest: serde_json::from_value(json!({
            "id": "ruff-cleanup",
            "title": "Ruff cleanup",
            "description": "Fix a large Ruff backlog in small agent batches."
        }))
        .expect("manifest"),
        script_path: PathBuf::from("/tmp/ruff.py"),
        source: "built-in".to_string(),
    }];
    chat.show_workflow_picker(definitions);

    assert_chatwidget_snapshot!(
        "workflow_picker",
        normalize_snapshot_paths(render_bottom_popup(&chat, /*width*/ 80))
    );
}

#[tokio::test]
async fn workflow_optional_text_field_can_submit_empty_value() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let field = serde_json::from_value(json!({
        "id": "model",
        "label": "Model",
        "type": "text",
        "default": ""
    }))
    .expect("field");
    chat.show_workflow_field("Demo", &field, 0, 1);

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::WorkflowFieldAnswered(answer)) if answer.is_empty()
    );
}

#[tokio::test]
async fn workflow_lifecycle_history_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.handle_workflow_update(&WorkflowUpdate::Started {
        run_id: "20260805-120000-12345678".to_string(),
        title: "Ruff cleanup".to_string(),
    });
    chat.handle_workflow_update(&WorkflowUpdate::Completed {
        run_id: "20260805-120000-12345678".to_string(),
        title: "Ruff cleanup".to_string(),
        result: json!({"passes": 12, "agent_calls": 48}),
        agent_calls: 48,
        shell_calls: 13,
    });

    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .flatten()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert_chatwidget_snapshot!("workflow_lifecycle_history", rendered);
}
