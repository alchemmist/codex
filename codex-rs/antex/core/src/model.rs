use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;
use serde_json::Value;

use crate::context::ContextFragment;
use crate::context::ToolOutput;

pub const MAX_TEXT_BYTES: usize = 8_000;
pub const MAX_SCHEMA_BYTES: usize = 8_000;
pub const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_STATE_BYTES: usize = 256 * 1024;
pub const MAX_TRANSCRIPT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TOOLS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Reasoning(String),
    Image { media_type: String, data: Arc<[u8]> },
    Continuation { provider: String, data: Arc<[u8]> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserInput {
    pub content: Vec<Content>,
    pub tool_scope: ToolScope,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ToolScope {
    #[default]
    Default,
    Named(String),
}

impl From<String> for UserInput {
    fn from(text: String) -> Self {
        Self {
            content: vec![Content::Text(text)],
            tool_scope: ToolScope::Default,
        }
    }
}

impl From<&str> for UserInput {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Context(ContextFragment),
    User(UserInput),
    Assistant {
        content: Vec<Content>,
        tool_calls: Vec<RawToolCall>,
    },
    Tool(ToolOutput),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub context_window: u64,
    pub reasoning_levels: Vec<String>,
    pub accepts_images: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRequest {
    pub model: String,
    pub reasoning: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelEvent {
    Quota(crate::Quota),
    Text(String),
    Reasoning(String),
    Continuation { provider: String, data: Arc<[u8]> },
    ToolCall(RawToolCall),
    Finished(Usage),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Authentication,
    RateLimited,
    Transport,
    Protocol,
    Limit,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub message: String,
}

pub type ModelStream = Pin<Box<dyn Stream<Item = Result<ModelEvent, ProviderError>> + Send>>;

/// Supplies provider-neutral model metadata and streams; implementations own authentication and wire conversion.
pub trait ModelProvider: Send + Sync {
    fn models(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send;
    fn stream(
        &self,
        request: ModelRequest,
    ) -> impl Future<Output = Result<ModelStream, ProviderError>> + Send;
}

impl<P: ModelProvider> ModelProvider for Arc<P> {
    fn models(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        self.as_ref().models()
    }
    fn stream(
        &self,
        request: ModelRequest,
    ) -> impl Future<Output = Result<ModelStream, ProviderError>> + Send {
        self.as_ref().stream(request)
    }
}
