use std::sync::Arc;
use std::sync::Mutex;

use antex_core::*;
use futures::StreamExt;
use pretty_assertions::assert_eq;

use super::Tools;
use super::drain;
use super::input;

struct InterruptAtCompletion(Arc<Mutex<Option<CommandSender>>>);

#[tokio::test]
async fn immediate_interrupt_preserves_the_submitted_request_and_queued_steers() {
    let provider = super::provider(Vec::new());
    let mut agent = Agent::new(provider.clone(), Arc::new(Tools::default()));
    let run = agent.start(input());
    let queued = AgentCommand::Steer("queued steering".into());
    run.commands.try_send(queued.clone()).unwrap();
    run.commands.try_send(AgentCommand::Interrupt).unwrap();
    assert_eq!(
        drain(run).await,
        vec![
            AgentEvent::MessageCommitted(Message::User("hello".into())),
            AgentEvent::Finished {
                reason: FinishReason::Interrupted,
                pending: vec![queued]
            }
        ]
    );
    assert!(provider.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn invalid_history_returns_the_uncommitted_request() {
    let mut agent = Agent::new(super::provider(Vec::new()), Arc::new(Tools::default()));
    let mut turn = input();
    turn.history
        .push(Message::User("x".repeat(MAX_TEXT_BYTES + 1).into()));
    let events = drain(agent.start(turn)).await;
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Failed,
            pending: vec![AgentCommand::FollowUp("hello".into())]
        })
    );
}

impl ModelProvider for InterruptAtCompletion {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }
    async fn stream(&self, _request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let commands = self.0.clone();
        let stream = futures::stream::iter(vec![
            Ok(ModelEvent::Text("finished text".into())),
            Ok(ModelEvent::Finished(Usage::default())),
        ])
        .inspect(move |event| {
            if matches!(event, Ok(ModelEvent::Finished(_))) {
                commands
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .try_send(AgentCommand::Interrupt)
                    .unwrap();
            }
        });
        Ok(Box::pin(stream))
    }
}

#[tokio::test]
async fn cancellation_at_completion_does_not_drop_already_streamed_assistant_text() {
    let control = Arc::new(Mutex::new(None));
    let mut agent = Agent::new(
        InterruptAtCompletion(control.clone()),
        Arc::new(Tools::default()),
    );
    let run = agent.start(input());
    *control.lock().unwrap() = Some(run.commands.clone());
    let events = drain(run).await;
    assert!(
        events.contains(&AgentEvent::MessageCommitted(Message::Assistant {
            content: vec![Content::Text("finished text".into())],
            tool_calls: Vec::new()
        }))
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Interrupted,
            pending: Vec::new()
        })
    );
}

struct RewritingContext;

impl ContextHook for RewritingContext {
    fn prepare<'a>(
        &'a self,
        _history: &'a [Message],
    ) -> futures::future::BoxFuture<'a, Result<PreparedContext, ProviderError>> {
        Box::pin(async { Ok(vec![Message::User("replacement".into())].into()) })
    }
}

#[tokio::test]
async fn context_hooks_cannot_rewrite_history_without_an_explicit_checkpoint() {
    let provider = super::provider(Vec::new());
    let mut agent = Agent::new(provider.clone(), Arc::new(Tools::default()))
        .with_context_hook(Arc::new(RewritingContext));
    let events = drain(agent.start(input())).await;
    assert!(provider.requests.lock().unwrap().is_empty());
    assert!(
        events.iter().any(
            |event| matches!(event,AgentEvent::Error(error) if error.kind==ErrorKind::Protocol)
        )
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Failed,
            pending: Vec::new()
        })
    );
}
