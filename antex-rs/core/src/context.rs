use crate::model::ErrorKind;
use crate::model::MAX_TEXT_BYTES;
use crate::model::Message;
use crate::model::ProviderError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextCheckpoint {
    pub summary: ContextFragment,
    pub retained: Vec<usize>,
    pub tail_start: usize,
    pub usage: crate::Usage,
}

pub struct PreparedContext {
    pub messages: Vec<Message>,
    pub checkpoint: Option<ContextCheckpoint>,
}

impl From<Vec<Message>> for PreparedContext {
    fn from(messages: Vec<Message>) -> Self {
        Self {
            messages,
            checkpoint: None,
        }
    }
}

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
    image: Option<crate::Content>,
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
            image: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl ToolOutput {
    pub fn with_image(mut self, image: crate::Content) -> Result<Self, ProviderError> {
        if self.text.len() > 1024
            || !matches!(&image,crate::Content::Image {media_type,data} if media_type.starts_with("image/") && media_type.len()<=128 && !data.is_empty() && data.len()<=crate::MAX_IMAGE_BYTES)
        {
            return Err(ProviderError {
                kind: ErrorKind::Limit,
                message: "invalid tool image or oversized caption".into(),
            });
        }
        self.image = Some(image);
        Ok(self)
    }

    pub fn image(&self) -> Option<&crate::Content> {
        self.image.as_ref()
    }
}

impl ContextualUserFragment for ToolOutput {
    fn to_message(&self) -> Message {
        Message::Tool(self.clone())
    }
}
