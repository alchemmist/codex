use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use antex_core::*;
use antex_runtime::LocalRuntime;
use antex_runtime::PermissionProfile;
use pretty_assertions::assert_eq;
use serde_json::json;

struct Provider(Mutex<VecDeque<Vec<ModelEvent>>>);

impl ModelProvider for Provider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }
    async fn stream(&self, _request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let events = self.0.lock().unwrap().pop_front().unwrap();
        Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
    }
}

fn call(id: &str, name: &str, args: serde_json::Value) -> ModelEvent {
    ModelEvent::ToolCall(RawToolCall {
        id: id.into(),
        name: name.into(),
        arguments: args.to_string(),
    })
}

fn input() -> TurnInput {
    TurnInput {
        model: "fake".into(),
        reasoning: None,
        history: Vec::new(),
        input: "complete the task".into(),
    }
}

#[tokio::test]
async fn agent_uses_real_file_tools_in_a_sequential_exchange() {
    let directory = tempfile::tempdir().unwrap();
    let runtime =
        Arc::new(LocalRuntime::new(directory.path(), PermissionProfile::Workspace).unwrap());
    let provider = Provider(Mutex::new(
        vec![
            vec![
                call(
                    "write",
                    "write",
                    json!({"path":"file","content":"hello\r\n"}),
                ),
                call(
                    "edit",
                    "edit",
                    json!({"path":"file","old_text":"hello","new_text":"world"}),
                ),
                call("read", "read", json!({"path":"file"})),
                ModelEvent::Finished(Usage::default()),
            ],
            vec![
                ModelEvent::Text("done".into()),
                ModelEvent::Finished(Usage::default()),
            ],
        ]
        .into(),
    ));
    let mut agent = Agent::new(provider, runtime);
    let mut run = agent.start(input());
    let mut outputs = Vec::new();
    while let Some(event) = run.events.recv().await {
        if let AgentEvent::MessageCommitted(Message::Tool(output)) = event {
            outputs.push(output);
        }
    }
    assert_eq!(
        outputs,
        vec![
            ToolOutput::new("write".into(), ToolOutcome::Success, "Wrote file".into()),
            ToolOutput::new("edit".into(), ToolOutcome::Success, "Edited file".into()),
            ToolOutput::new("read".into(), ToolOutcome::Success, "1: world\n".into()),
        ]
    );
    assert_eq!(
        std::fs::read(directory.path().join("file")).unwrap(),
        b"world\r\n"
    );
}

#[tokio::test]
async fn readonly_registry_prevents_the_model_from_calling_write() {
    let directory = tempfile::tempdir().unwrap();
    let runtime =
        Arc::new(LocalRuntime::new(directory.path(), PermissionProfile::ReadOnly).unwrap());
    let provider = Provider(Mutex::new(
        vec![
            vec![
                call(
                    "write",
                    "write",
                    json!({"path":"file","content":"forbidden"}),
                ),
                ModelEvent::Finished(Usage::default()),
            ],
            vec![ModelEvent::Finished(Usage::default())],
        ]
        .into(),
    ));
    let mut agent = Agent::new(provider, runtime);
    let mut run = agent.start(input());
    let mut outputs = Vec::new();
    while let Some(event) = run.events.recv().await {
        if let AgentEvent::MessageCommitted(Message::Tool(output)) = event {
            outputs.push(output);
        }
    }
    assert_eq!(
        outputs,
        vec![ToolOutput::new(
            "write".into(),
            ToolOutcome::Failure,
            "tool is not enabled for this turn".into()
        )]
    );
    assert!(!directory.path().join("file").exists());
}

#[tokio::test]
async fn unsandboxed_shell_requires_a_fresh_explicit_approval() {
    for answer in [InteractionAnswer::AllowOnce, InteractionAnswer::Deny] {
        let directory = tempfile::tempdir().unwrap();
        let runtime =
            Arc::new(LocalRuntime::new(directory.path(), PermissionProfile::Workspace).unwrap());
        let provider = Provider(Mutex::new(
            vec![
                vec![
                    call(
                        "shell",
                        "shell",
                        json!({"command":"printf approved > file","access":"requestFull"}),
                    ),
                    ModelEvent::Finished(Usage::default()),
                ],
                vec![ModelEvent::Finished(Usage::default())],
            ]
            .into(),
        ));
        let mut agent = Agent::new(provider, runtime);
        let mut run = agent.start(input());
        let mut approvals = 0;
        while let Some(event) = run.events.recv().await {
            if let AgentEvent::Interaction { request, .. } = event {
                approvals += 1;
                assert_eq!(
                    request.prompt(),
                    &InteractionPrompt::Approval {
                        action:
                            "Approve this command once outside the sandbox:\nprintf approved > file"
                                .into()
                    }
                );
                request.answer(answer.clone()).unwrap();
            }
        }
        assert_eq!(approvals, 1);
        assert_eq!(
            directory.path().join("file").exists(),
            answer == InteractionAnswer::AllowOnce
        );
    }
}

#[tokio::test]
async fn force_push_still_requires_confirmation_in_full_profile() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = Arc::new(LocalRuntime::new(directory.path(), PermissionProfile::Full).unwrap());
    let provider = Provider(Mutex::new(
        vec![
            vec![
                call(
                    "shell",
                    "shell",
                    json!({"command":"git push --force-with-lease origin main"}),
                ),
                ModelEvent::Finished(Usage::default()),
            ],
            vec![ModelEvent::Finished(Usage::default())],
        ]
        .into(),
    ));
    let mut agent = Agent::new(provider, runtime);
    let mut run = agent.start(input());
    let mut denied = Vec::new();
    while let Some(event) = run.events.recv().await {
        match event {
            AgentEvent::Interaction { request, .. } => {
                request.answer(InteractionAnswer::Deny).unwrap()
            }
            AgentEvent::MessageCommitted(Message::Tool(output)) => denied.push(output),
            _ => {}
        }
    }
    assert_eq!(
        denied,
        vec![ToolOutput::new(
            "shell".into(),
            ToolOutcome::Failure,
            "command denied".into()
        )]
    );
}
