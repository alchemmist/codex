use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use antex_core::*;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;

use super::Tools;
use super::call;
use super::input;
use super::provider;

struct ApprovingTools {
    executions: AtomicUsize,
}

impl ToolHost for ApprovingTools {
    fn definitions(&self, scope: &ToolScope) -> Vec<ToolDefinition> {
        Tools::default().definitions(scope)
    }

    fn execute(&self, call: ToolCall, context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let result = context
                .interact(InteractionPrompt::Approval {
                    action: "run command".into(),
                })
                .await;
            match result {
                Ok(InteractionAnswer::AllowOnce) => {
                    self.executions.fetch_add(1, Ordering::SeqCst);
                    ToolOutput::new(call.id, ToolOutcome::Success, "approved".into())
                }
                Ok(InteractionAnswer::Deny | InteractionAnswer::Text(_)) => {
                    ToolOutput::new(call.id, ToolOutcome::Failure, "denied".into())
                }
                Err(_) => ToolOutput::new(call.id, ToolOutcome::Cancelled, "cancelled".into()),
            }
        })
    }
}

#[tokio::test]
async fn approval_is_one_shot_and_never_enters_the_model_transcript() {
    let provider = provider(vec![
        vec![
            call("echo", r#"{"text":"hello"}"#),
            ModelEvent::Finished(Usage::default()),
        ],
        vec![ModelEvent::Finished(Usage::default())],
    ]);
    let tools = Arc::new(ApprovingTools {
        executions: AtomicUsize::new(0),
    });
    let mut agent = Agent::new(provider.clone(), tools.clone());
    let mut run = agent.start(input());
    while let Some(event) = run.events.recv().await {
        if let AgentEvent::Interaction { request, .. } = event {
            assert_eq!(
                request.prompt(),
                &InteractionPrompt::Approval {
                    action: "run command".into()
                }
            );
            assert!(
                request
                    .answer(InteractionAnswer::Text("yes".into()))
                    .is_err()
            );
            request.answer(InteractionAnswer::AllowOnce).unwrap();
            assert!(request.answer(InteractionAnswer::AllowOnce).is_err());
        }
    }
    assert_eq!(tools.executions.load(Ordering::SeqCst), 1);
    let requests = provider.requests.lock().unwrap();
    assert_eq!(
        requests[1].messages.last(),
        Some(&Message::Tool(ToolOutput::new(
            "call-1".into(),
            ToolOutcome::Success,
            "approved".into()
        )))
    );
    assert_eq!(requests[1].messages.len(), 3);
}

#[tokio::test]
async fn interrupt_revokes_an_unanswered_approval_without_executing_the_tool() {
    let provider = provider(vec![vec![
        call("echo", r#"{"text":"hello"}"#),
        ModelEvent::Finished(Usage::default()),
    ]]);
    let tools = Arc::new(ApprovingTools {
        executions: AtomicUsize::new(0),
    });
    let mut agent = Agent::new(provider, tools.clone());
    let mut run = agent.start(input());
    let mut pending = None;
    let mut finished = None;
    while let Some(event) = run.events.recv().await {
        match event {
            AgentEvent::Interaction { request, .. } => {
                pending = Some(request);
                run.commands.send(AgentCommand::Interrupt).await.unwrap();
            }
            AgentEvent::Finished { reason, .. } => finished = Some(reason),
            _ => {}
        }
    }
    assert_eq!(tools.executions.load(Ordering::SeqCst), 0);
    assert_eq!(finished, Some(FinishReason::Interrupted));
    assert!(
        pending
            .unwrap()
            .answer(InteractionAnswer::AllowOnce)
            .is_err()
    );
}
