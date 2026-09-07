use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::AgentEvent;
use crate::Content;
use crate::ErrorKind;
use crate::Message;
use crate::ModelEvent;
use crate::ModelProvider;
use crate::ModelRequest;
use crate::ProviderError;
use crate::RawToolCall;
use crate::Usage;
use crate::validation;

pub(crate) struct Response {
    pub content: Vec<Content>,
    pub calls: Vec<RawToolCall>,
    pub usage: Usage,
}

pub(crate) use crate::validation::error;

pub(crate) async fn emit(
    events: &mpsc::Sender<AgentEvent>,
    cancel: &CancellationToken,
    event: AgentEvent,
) -> Result<(), ProviderError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(error(ErrorKind::Cancelled, "run interrupted")),
        result = events.send(event) => result.map_err(|_| error(ErrorKind::Cancelled, "event receiver closed")),
    }
}

pub(crate) async fn collect<P: ModelProvider>(
    provider: &P,
    request: ModelRequest,
    events: &mpsc::Sender<AgentEvent>,
    cancel: &CancellationToken,
    response: &mut Response,
) -> Result<(), ProviderError> {
    let mut attempts = 0;
    let mut stream = open(provider, &request, events, cancel, &mut attempts).await?;
    loop {
        let item = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(error(ErrorKind::Cancelled, "run interrupted")),
            _ = events.closed() => return Err(error(ErrorKind::Cancelled, "event receiver closed")),
            item = stream.next() => item,
        };
        let item = match item {
            Some(Ok(item)) => item,
            Some(Err(error))
                if attempts < 2
                    && response.content.is_empty()
                    && response.calls.is_empty()
                    && matches!(error.kind, ErrorKind::Transport | ErrorKind::RateLimited) =>
            {
                attempts += 1;
                retry_delay(events, cancel, attempts).await?;
                stream = open(provider, &request, events, cancel, &mut attempts).await?;
                continue;
            }
            Some(Err(error)) => return Err(error),
            None => {
                return Err(error(
                    ErrorKind::Protocol,
                    "model stream ended without completion",
                ));
            }
        };
        let previous_content = response.content.clone();
        let event = match item {
            ModelEvent::Quota(quota) => {
                if quota
                    .credits
                    .as_ref()
                    .and_then(|credits| credits.balance.as_ref())
                    .is_some_and(|balance| balance.len() > 64)
                {
                    return Err(error(
                        ErrorKind::Limit,
                        "quota metadata exceeds its byte budget",
                    ));
                }
                Some(AgentEvent::Quota(quota))
            }
            ModelEvent::Text(text) => {
                if let Some(Content::Text(previous)) = response.content.last_mut() {
                    if previous.len() + text.len() > crate::MAX_TEXT_BYTES {
                        return Err(error(
                            ErrorKind::Limit,
                            "assistant text exceeds its byte budget",
                        ));
                    }
                    previous.push_str(&text);
                } else {
                    response.content.push(Content::Text(text.clone()));
                }
                Some(AgentEvent::TextDelta(text))
            }
            ModelEvent::Reasoning(text) => {
                if let Some(Content::Reasoning(previous)) = response.content.last_mut() {
                    if previous.len() + text.len() > crate::MAX_TEXT_BYTES {
                        return Err(error(
                            ErrorKind::Limit,
                            "assistant reasoning exceeds its byte budget",
                        ));
                    }
                    previous.push_str(&text);
                } else {
                    response.content.push(Content::Reasoning(text.clone()));
                }
                Some(AgentEvent::ReasoningDelta(text))
            }
            ModelEvent::Continuation { provider, data } => {
                response
                    .content
                    .push(Content::Continuation { provider, data });
                None
            }
            ModelEvent::ToolCall(call) => {
                if response.calls.iter().any(|previous| previous.id == call.id) {
                    return Err(error(ErrorKind::Protocol, "duplicate tool call identifier"));
                }
                response.calls.push(call);
                None
            }
            ModelEvent::Finished(usage) => {
                response.usage = usage;
                return Ok(());
            }
        };
        if let Err(error) = validation::messages(&[Message::Assistant {
            content: response.content.clone(),
            tool_calls: response.calls.clone(),
        }]) {
            response.content = previous_content;
            return Err(error);
        }
        if let Some(event) = event {
            emit(events, cancel, event).await?;
        }
    }
}

async fn open<P: ModelProvider>(
    provider: &P,
    request: &ModelRequest,
    events: &mpsc::Sender<AgentEvent>,
    cancel: &CancellationToken,
    attempts: &mut u64,
) -> Result<crate::ModelStream, ProviderError> {
    loop {
        let result = tokio::select! {
            biased;
            _=cancel.cancelled()=>return Err(error(ErrorKind::Cancelled,"run interrupted")),
            _=events.closed()=>return Err(error(ErrorKind::Cancelled,"event receiver closed")),
            result=provider.stream(request.clone())=>result,
        };
        match result {
            Ok(stream) => return Ok(stream),
            Err(error)
                if *attempts < 2
                    && matches!(error.kind, ErrorKind::Transport | ErrorKind::RateLimited) =>
            {
                *attempts += 1;
                retry_delay(events, cancel, *attempts).await?;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn retry_delay(
    events: &mpsc::Sender<AgentEvent>,
    cancel: &CancellationToken,
    attempt: u64,
) -> Result<(), ProviderError> {
    tokio::select! {
        _=cancel.cancelled()=>Err(error(ErrorKind::Cancelled,"run interrupted")),
        _=events.closed()=>Err(error(ErrorKind::Cancelled,"event receiver closed")),
        _=tokio::time::sleep(std::time::Duration::from_millis(100*attempt))=>Ok(()),
    }
}
