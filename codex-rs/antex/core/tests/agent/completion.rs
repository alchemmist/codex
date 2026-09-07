use std::sync::Arc;
use std::sync::Mutex;

use antex_core::*;
use futures::StreamExt;
use pretty_assertions::assert_eq;

use super::Tools;
use super::drain;
use super::input;

struct InterruptAtCompletion(Arc<Mutex<Option<CommandSender>>>);

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
