use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use antex_core::*;
use pretty_assertions::assert_eq;

use super::Tools;
use super::drain;
use super::input;

enum Reply {
    OpenFailure,
    StreamFailure,
    PartialFailure,
    Success,
}

struct Provider {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ModelProvider for Provider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }
    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let error = ProviderError {
            kind: ErrorKind::Transport,
            message: "disconnected".into(),
        };
        let events = match self.replies.lock().unwrap().pop_front().unwrap() {
            Reply::OpenFailure => return Err(error),
            Reply::StreamFailure => vec![Err(error)],
            Reply::PartialFailure => vec![Ok(ModelEvent::Text("partial".into())), Err(error)],
            Reply::Success => vec![
                Ok(ModelEvent::Text("done".into())),
                Ok(ModelEvent::Finished(Usage::default())),
            ],
        };
        Ok(Box::pin(futures::stream::iter(events)))
    }
}

#[tokio::test]
async fn handshake_and_empty_stream_retries_share_one_bounded_budget() {
    let provider = Arc::new(Provider {
        replies: Mutex::new(vec![Reply::OpenFailure, Reply::StreamFailure, Reply::Success].into()),
        requests: Mutex::new(Vec::new()),
    });
    let mut agent = Agent::new(provider.clone(), Arc::new(Tools::default()));
    let events = drain(agent.start(input())).await;
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(&requests[1..], &[requests[0].clone(), requests[0].clone()]);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TextDelta(_)))
            .count(),
        1
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Completed,
            pending: Vec::new()
        })
    );
}

#[tokio::test]
async fn retry_exhaustion_stops_before_a_fourth_request() {
    let provider = Arc::new(Provider {
        replies: Mutex::new(
            vec![
                Reply::StreamFailure,
                Reply::OpenFailure,
                Reply::StreamFailure,
                Reply::Success,
            ]
            .into(),
        ),
        requests: Mutex::new(Vec::new()),
    });
    let mut agent = Agent::new(provider.clone(), Arc::new(Tools::default()));
    let events = drain(agent.start(input())).await;
    assert_eq!(provider.requests.lock().unwrap().len(), 3);
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Failed,
            pending: Vec::new()
        })
    );
}

#[tokio::test]
async fn partial_output_is_not_replayed_by_an_automatic_retry() {
    let provider = Arc::new(Provider {
        replies: Mutex::new(vec![Reply::PartialFailure, Reply::Success].into()),
        requests: Mutex::new(Vec::new()),
    });
    let mut agent = Agent::new(provider.clone(), Arc::new(Tools::default()));
    let events = drain(agent.start(input())).await;
    assert_eq!(provider.requests.lock().unwrap().len(), 1);
    assert!(
        events.contains(&AgentEvent::MessageCommitted(Message::Assistant {
            content: vec![Content::Text("partial".into())],
            tool_calls: Vec::new()
        }))
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Failed,
            pending: Vec::new()
        })
    );
}
