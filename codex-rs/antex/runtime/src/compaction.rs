use std::sync::Arc;
use std::sync::Mutex;

use antex_core::Content;
use antex_core::ContextCheckpoint;
use antex_core::ContextFragment;
use antex_core::ContextHook;
use antex_core::ContextKind;
use antex_core::ContextualUserFragment;
use antex_core::ErrorKind;
use antex_core::Message;
use antex_core::ModelEvent;
use antex_core::ModelProvider;
use antex_core::ModelRequest;
use antex_core::PreparedContext;
use antex_core::ProviderError;
use antex_core::Usage;
use futures::StreamExt;
use futures::future::BoxFuture;
use sha2::Digest;
use sha2::Sha256;

use crate::ProjectContext;
use crate::session_codec;

pub struct Compaction<P> {
    provider: Arc<P>,
    project: ProjectContext,
    model: String,
    token_limit: usize,
    state: Mutex<Option<Cached>>,
}

#[derive(Clone)]
struct Cached {
    checkpoint: ContextCheckpoint,
    digest: [u8; 32],
}

impl<P: ModelProvider> Compaction<P> {
    pub fn new(
        provider: Arc<P>,
        project: ProjectContext,
        model: String,
        token_limit: usize,
    ) -> Self {
        Self {
            provider,
            project,
            model,
            token_limit,
            state: Mutex::new(None),
        }
    }

    pub async fn compact(&self, history: &[Message]) -> Result<ContextCheckpoint, ProviderError> {
        let prepared = self.project.prepare(history).await?.messages;
        self.summarize(history, prepared).await
    }

    async fn summarize(
        &self,
        history: &[Message],
        mut messages: Vec<Message>,
    ) -> Result<ContextCheckpoint, ProviderError> {
        let mut start = history.len().saturating_sub(8);
        while start > 0 && matches!(history[start], Message::Tool(_)) {
            start -= 1;
        }
        if start == 0 {
            return Err(failure("not enough completed history to compact safely"));
        }
        let retained = history
            .iter()
            .rposition(|message| matches!(message, Message::User(_)))
            .filter(|index| *index < start)
            .into_iter()
            .collect::<Vec<_>>();
        messages.push(Message::User("Summarize this coding session for continuation. Preserve the user's goal, decisions, changed files, important tool outcomes, and unresolved work. Do not execute tools or continue the task. Return only a concise summary under 6000 UTF-8 bytes.".into()));
        let request = ModelRequest {
            model: self.model.clone(),
            reasoning: None,
            messages,
            tools: Vec::new(),
        };
        let mut attempts = 0;
        let mut stream = loop {
            match self.provider.stream(request.clone()).await {
                Ok(stream) => break stream,
                Err(error)
                    if attempts < 2
                        && matches!(error.kind, ErrorKind::Transport | ErrorKind::RateLimited) =>
                {
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(100 * attempts)).await;
                }
                Err(error) => return Err(error),
            }
        };
        let mut text = String::new();
        while let Some(event) = stream.next().await {
            match event? {
                ModelEvent::Text(delta) => {
                    if text.len() + delta.len() > antex_core::MAX_TEXT_BYTES {
                        return Err(failure("summary exceeds its byte budget"));
                    }
                    text.push_str(&delta);
                }
                ModelEvent::Finished(usage) => {
                    if text.trim().is_empty() {
                        return Err(failure("provider returned an empty summary"));
                    }
                    let summary = ContextFragment::new(ContextKind::Summary, text)?;
                    return Ok(ContextCheckpoint {
                        summary,
                        retained,
                        tail_start: start,
                        usage,
                    });
                }
                ModelEvent::ToolCall(_) => return Err(failure("summary attempted to call a tool")),
                ModelEvent::Reasoning(_)
                | ModelEvent::Continuation { .. }
                | ModelEvent::Quota(_) => {}
            }
        }
        Err(failure("summary stream ended without completion"))
    }
}

impl<P: ModelProvider> ContextHook for Compaction<P> {
    fn prepare<'a>(
        &'a self,
        history: &'a [Message],
    ) -> BoxFuture<'a, Result<PreparedContext, ProviderError>> {
        Box::pin(async move {
            let state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
                .filter(|cached| {
                    cached.checkpoint.tail_start < history.len()
                        && digest(&history[..cached.checkpoint.tail_start]) == cached.digest
                });
            let effective = match &state {
                Some(cached) => project(history, &cached.checkpoint),
                None => history.to_vec(),
            };
            let mut messages = self.project.prepare(&effective).await?.messages;
            let mut checkpoint = state.as_ref().map(|cached| ContextCheckpoint {
                usage: Usage::default(),
                ..cached.checkpoint.clone()
            });
            if estimate(&messages) > self.token_limit {
                let next = self.summarize(history, messages).await?;
                messages = self
                    .project
                    .prepare(&project(history, &next))
                    .await?
                    .messages;
                if estimate(&messages) > self.token_limit {
                    return Err(failure(
                        "recent context and instructions exceed the configured context limit",
                    ));
                }
                *self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Cached {
                    digest: digest(&history[..next.tail_start]),
                    checkpoint: next.clone(),
                });
                checkpoint = Some(next);
            }
            Ok(PreparedContext {
                messages,
                checkpoint,
            })
        })
    }
}

fn project(history: &[Message], checkpoint: &ContextCheckpoint) -> Vec<Message> {
    std::iter::once(checkpoint.summary.to_message())
        .chain(
            checkpoint
                .retained
                .iter()
                .map(|index| history[*index].clone()),
        )
        .chain(history[checkpoint.tail_start..].iter().cloned())
        .collect()
}

fn digest(messages: &[Message]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for message in messages {
        let encoded = session_codec::encode(message).to_string();
        hash.update(encoded.len().to_le_bytes());
        hash.update(encoded.as_bytes());
    }
    hash.finalize().into()
}

fn estimate(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| {
            32 + match message {
                Message::Context(fragment) => fragment.text().len().div_ceil(3),
                Message::User(input) => content_estimate(&input.content),
                Message::Assistant {
                    content,
                    tool_calls,
                } => {
                    content_estimate(content)
                        + tool_calls
                            .iter()
                            .map(|call| call.arguments.len().div_ceil(3) + 32)
                            .sum::<usize>()
                }
                Message::Tool(output) => {
                    output.text().len().div_ceil(3)
                        + output
                            .image()
                            .map(|image| content_estimate(std::slice::from_ref(image)))
                            .unwrap_or_default()
                }
            }
        })
        .sum()
}

fn content_estimate(content: &[Content]) -> usize {
    content
        .iter()
        .map(|block| match block {
            Content::Text(text) | Content::Reasoning(text) => text.len().div_ceil(3),
            Content::Continuation { data, .. } => data.len().div_ceil(3),
            Content::Image { .. } => 8192,
        })
        .sum()
}

fn failure(message: &str) -> ProviderError {
    ProviderError {
        kind: ErrorKind::Limit,
        message: message.into(),
    }
}
