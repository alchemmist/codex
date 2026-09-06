use futures::future::BoxFuture;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::AgentCommand;
use crate::CommandSender;
use crate::Message;
use crate::ProviderError;
use crate::ToolCall;
use crate::ToolDefinition;
use crate::ToolOutput;
use crate::ToolScope;
use crate::Usage;
use crate::UserInput;

pub struct TurnInput {
    pub model: String,
    pub reasoning: Option<String>,
    pub history: Vec<Message>,
    pub input: UserInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    MessageCommitted(Message),
    TextDelta(String),
    ReasoningDelta(String),
    ToolStarted(ToolCall),
    Usage(Usage),
    TurnCompleted,
    Error(ProviderError),
    Finished { reason: FinishReason, pending: Vec<AgentCommand> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishReason {
    Completed,
    Interrupted,
    Failed,
}

pub struct AgentRun {
    pub events: mpsc::Receiver<AgentEvent>,
    pub commands: CommandSender,
}

pub struct ToolContext {
    pub cancellation: CancellationToken,
}

/// Resolves the enabled tool set and executes validated calls; implementations enforce permissions and cancellation.
pub trait ToolHost: Send + Sync {
    fn definitions(&self, scope: &ToolScope) -> Vec<ToolDefinition>;
    fn execute(&self, call: ToolCall, context: ToolContext) -> BoxFuture<'_, ToolOutput>;
}

/// Prepares bounded request context without modifying the committed transcript or enabled tools.
pub trait ContextHook: Send + Sync {
    fn prepare<'a>(&'a self, history: &'a [Message]) -> BoxFuture<'a, Result<Vec<Message>, ProviderError>>;
}
