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
