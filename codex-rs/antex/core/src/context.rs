use crate::model::ErrorKind;
use crate::model::MAX_TEXT_BYTES;
use crate::model::Message;
use crate::model::ProviderError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextKind {
    System,
    Project,
    Skill,
    Summary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextFragment {
    kind: ContextKind,
    text: String,
}

impl ContextFragment {
    pub fn new(kind: ContextKind, text: String) -> Result<Self, ProviderError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(ProviderError {
                kind: ErrorKind::Limit,
                message: "context fragment exceeds its byte budget".into(),
            });
        }
        Ok(Self { kind, text })
    }

    pub fn kind(&self) -> ContextKind {
        self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Converts an already bounded context fragment into a model message without editing prior messages.
pub trait ContextualUserFragment {
    fn to_message(&self) -> Message;
}

impl ContextualUserFragment for ContextFragment {
    fn to_message(&self) -> Message {
        Message::Context(self.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolOutcome {
    Success,
    Failure,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolOutput {
    pub call_id: String,
    pub outcome: ToolOutcome,
    text: String,
}

impl ToolOutput {
    pub fn new(call_id: String, outcome: ToolOutcome, mut text: String) -> Self {
        const MARKER: &str = "\n[output truncated]";
        if text.len() > MAX_TEXT_BYTES {
            let mut end = MAX_TEXT_BYTES - MARKER.len();
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str(MARKER);
        }
        Self {
            call_id,
            outcome,
            text,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl ContextualUserFragment for ToolOutput {
    fn to_message(&self) -> Message {
        Message::Tool(self.clone())
    }
}
