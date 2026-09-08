use std::future::Future;

use antex_core::AgentEvent;
use antex_core::AgentRun;
use antex_core::Message;
use antex_core::UserInput;

pub struct SessionView {
    pub model: String,
    pub directory: std::path::PathBuf,
    pub permissions: String,
    pub session_id: String,
}

pub enum CommandEffect {
    Notice(String),
    Reset(Vec<Message>),
    Image(antex_core::Content),
    Picker(crate::PickerSpec),
    Page(crate::TextPage),
}

/// Composes runtime persistence and agent runs without exposing provider or filesystem internals to the terminal.
pub trait Session: Send {
    fn background_notice(&mut self) -> impl Future<Output = Result<String, String>> + Send {
        std::future::pending()
    }
    fn view(&self) -> SessionView;
    fn history(&self) -> Vec<Message>;
    fn pending_commands(&mut self) -> Result<Vec<antex_core::AgentCommand>, String>;
    fn queue_command(&mut self, command: &antex_core::AgentCommand) -> Result<(), String>;
    fn load_ui_state(&mut self, name: &str) -> Result<Option<serde_json::Value>, String>;
    fn save_ui_state(&mut self, name: &str, value: &serde_json::Value) -> Result<(), String>;
    fn prepare_image(
        &mut self,
        source: crate::ImageSource,
    ) -> impl Future<Output = Result<antex_core::Content, String>> + Send;
    fn start(&mut self, input: UserInput) -> impl Future<Output = Result<AgentRun, String>> + Send;
    fn record(&mut self, event: &AgentEvent) -> impl Future<Output = Result<(), String>> + Send;
    fn command(
        &mut self,
        command: &str,
    ) -> impl Future<Output = Result<CommandEffect, String>> + Send;
}
