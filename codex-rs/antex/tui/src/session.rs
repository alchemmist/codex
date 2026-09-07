use std::future::Future;

use antex_core::AgentEvent;
use antex_core::AgentRun;
use antex_core::Message;
use antex_core::UserInput;

pub struct SessionView {
    pub model: String,
    pub directory: String,
    pub permissions: String,
    pub session_id: String,
}

pub enum CommandEffect {
    Notice(String),
    Reset(Vec<Message>),
}

/// Composes runtime persistence and agent runs without exposing provider or filesystem internals to the terminal.
pub trait Session: Send {
    fn view(&self) -> SessionView;
    fn history(&self) -> Vec<Message>;
    fn pending_commands(&mut self) -> Result<Vec<antex_core::AgentCommand>, String>;
    fn start(&mut self, input: UserInput) -> impl Future<Output = Result<AgentRun, String>> + Send;
    fn record(&mut self, event: &AgentEvent) -> Result<(), String>;
    fn command(
        &mut self,
        command: &str,
    ) -> impl Future<Output = Result<CommandEffect, String>> + Send;
}
