use super::*;
use crate::workflow::WorkflowDefinition;
use crate::workflow::WorkflowUpdate;

#[tokio::test]
async fn workflow_picker_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let definitions = [
        (
            "github-bot-pr-maintenance",
            "GitHub bot PR maintenance",
            "Review or safely merge bot pull requests across owned GitHub repositories.",
            "/tmp/github.py",
        ),
        (
            "ruff-cleanup",
            "Ruff cleanup",
            "Fix a large Ruff backlog in small agent batches.",
            "/tmp/ruff.py",
        ),
    ]
    .into_iter()
    .map(|(id, title, description, script_path)| WorkflowDefinition {
        manifest: serde_json::from_value(json!({
            "id": id,
            "title": title,
            "description": description,
        }))
        .expect("manifest"),
        script_path: PathBuf::from(script_path),
        source: "built-in".to_string(),
    })
    .collect();
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

#[tokio::test]
async fn workflow_agent_status_retains_current_phase_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let run_id = "20260825-161943-f2b3fe76".to_string();
    chat.handle_workflow_update(&WorkflowUpdate::Started {
        run_id: run_id.clone(),
        title: "GitHub bot PR maintenance".to_string(),
    });
    chat.handle_workflow_update(&WorkflowUpdate::Progress {
        run_id: run_id.clone(),
        message: "Repositories 6-10: alpha, beta, gamma, delta, epsilon".to_string(),
        current: Some(5),
        total: Some(42),
    });
    chat.handle_workflow_update(&WorkflowUpdate::AgentFinished {
        run_id,
        completed: 3,
        total: 5,
        success: true,
        phase: Some("Repositories 6-10: alpha, beta, gamma, delta, epsilon".to_string()),
        phase_current: Some(5),
        phase_total: Some(42),
    });

    assert_chatwidget_snapshot!(
        "workflow_agent_status_with_phase",
        render_bottom_popup(&chat, /*width*/ 96)
    );
}
