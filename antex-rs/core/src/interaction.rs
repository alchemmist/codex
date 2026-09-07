use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::AgentEvent;
use crate::ErrorKind;
use crate::MAX_TEXT_BYTES;
use crate::ProviderError;
use crate::response::emit;
use crate::validation::error;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InteractionPrompt {
    Approval {
        action: String,
    },
    Question {
        question: String,
        choices: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InteractionAnswer {
    AllowOnce,
    Deny,
    Text(String),
}

#[derive(Clone)]
pub struct Interaction {
    id: u64,
    prompt: InteractionPrompt,
    answer: Arc<Mutex<Option<oneshot::Sender<InteractionAnswer>>>>,
}

impl std::fmt::Debug for Interaction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Interaction")
            .field("id", &self.id)
            .field("prompt", &self.prompt)
            .finish()
    }
}

impl PartialEq for Interaction {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.prompt == other.prompt
    }
}

impl Eq for Interaction {}

impl Interaction {
    pub fn prompt(&self) -> &InteractionPrompt {
        &self.prompt
    }

    pub fn answer(&self, answer: InteractionAnswer) -> Result<(), ProviderError> {
        let valid = match (&self.prompt, &answer) {
            (
                InteractionPrompt::Approval { .. },
                InteractionAnswer::AllowOnce | InteractionAnswer::Deny,
            ) => true,
            (InteractionPrompt::Question { choices, .. }, InteractionAnswer::Text(text)) => {
                text.len() <= MAX_TEXT_BYTES && (choices.is_empty() || choices.contains(text))
            }
            (InteractionPrompt::Approval { .. }, InteractionAnswer::Text(_))
            | (
                InteractionPrompt::Question { .. },
                InteractionAnswer::AllowOnce | InteractionAnswer::Deny,
            ) => false,
        };
        if !valid {
            return Err(error(
                ErrorKind::Protocol,
                "answer does not match the pending interaction",
            ));
        }
        self.answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| error(ErrorKind::Cancelled, "interaction is already answered"))?
            .send(answer)
            .map_err(|_| error(ErrorKind::Cancelled, "interaction is no longer active"))
    }
}

pub struct ToolContext {
    pub cancellation: CancellationToken,
    pub(crate) events: mpsc::Sender<AgentEvent>,
    pub(crate) call_id: String,
}

impl ToolContext {
    pub async fn progress(&self, text: String) -> Result<(), ProviderError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(error(
                ErrorKind::Limit,
                "tool progress exceeds its byte budget",
            ));
        }
        emit(
            &self.events,
            &self.cancellation,
            AgentEvent::ToolProgress {
                call_id: self.call_id.clone(),
                text,
            },
        )
        .await
    }

    pub async fn interact(
        &self,
        prompt: InteractionPrompt,
    ) -> Result<InteractionAnswer, ProviderError> {
        let valid = match &prompt {
            InteractionPrompt::Approval { action } => action.len() <= MAX_TEXT_BYTES,
            InteractionPrompt::Question { question, choices } => {
                question.len() <= MAX_TEXT_BYTES
                    && choices.len() <= 8
                    && choices.iter().all(|choice| choice.len() <= 128)
            }
        };
        if !valid {
            return Err(error(
                ErrorKind::Limit,
                "interaction exceeds its size budget",
            ));
        }
        let (sender, receiver) = oneshot::channel();
        let interaction = Interaction {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            prompt,
            answer: Arc::new(Mutex::new(Some(sender))),
        };
        emit(
            &self.events,
            &self.cancellation,
            AgentEvent::Interaction {
                call_id: self.call_id.clone(),
                request: interaction,
            },
        )
        .await?;
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(error(ErrorKind::Cancelled, "interaction cancelled")),
            answer = receiver => answer.map_err(|_| error(ErrorKind::Cancelled, "interaction dismissed")),
        }
    }
}
