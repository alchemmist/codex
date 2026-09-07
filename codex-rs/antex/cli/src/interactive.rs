use std::path::PathBuf;
use std::sync::Arc;

use antex_core::Agent;
use antex_core::AgentEvent;
use antex_core::AgentRun;
use antex_core::Message;
use antex_core::ModelProvider;
use antex_core::TurnInput;
use antex_core::UserInput;
use antex_provider_openai::OpenAiProvider;
use antex_runtime::Compaction;
use antex_runtime::Config;
use antex_runtime::Conversation;
use antex_runtime::LocalRuntime;
use antex_runtime::ProjectContext;
use antex_runtime::SessionStore;
use antex_tui::CommandEffect;
use antex_tui::Session;
use antex_tui::SessionView;

pub(crate) struct InteractiveSession {
    home: PathBuf,
    workspace: PathBuf,
    config: Config,
    bubblewrap: Option<PathBuf>,
    provider: Arc<OpenAiProvider>,
    context: ProjectContext,
    conversation: Conversation,
    agent: Option<Agent<Arc<OpenAiProvider>>>,
    compaction: Option<Arc<Compaction<OpenAiProvider>>>,
}

impl InteractiveSession {
    pub(crate) fn new(
        home: PathBuf,
        workspace: PathBuf,
        config: Config,
        bubblewrap: Option<PathBuf>,
        provider: OpenAiProvider,
    ) -> Result<Self, String> {
        let context = ProjectContext::load(&home, &workspace).map_err(|error| error.to_string())?;
        let store = SessionStore::new(&home, &workspace).map_err(|error| error.to_string())?;
        let conversation = Conversation::new(store.create().map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            home,
            workspace,
            config,
            bubblewrap,
            provider: Arc::new(provider),
            context,
            conversation,
            agent: None,
            compaction: None,
        })
    }

    fn reset_agent(&mut self) {
        self.agent = None;
        self.compaction = None;
    }
}

impl Session for InteractiveSession {
    fn load_ui_state(&mut self, name: &str) -> Result<Option<serde_json::Value>, String> {
        self.conversation
            .load_ui_state(name)
            .map_err(|error| error.to_string())
    }
    fn save_ui_state(&mut self, name: &str, value: &serde_json::Value) -> Result<(), String> {
        self.conversation
            .save_ui_state(name, value)
            .map_err(|error| error.to_string())
    }
    fn view(&self) -> SessionView {
        SessionView {
            model: self
                .config
                .model
                .clone()
                .unwrap_or_else(|| "select model on first turn".into()),
            directory: self.workspace.clone(),
            permissions: format!("{:?}", self.config.permissions),
            session_id: self.conversation.id().to_string(),
        }
    }

    fn history(&self) -> Vec<Message> {
        self.conversation.messages()
    }

    fn pending_commands(&mut self) -> Result<Vec<antex_core::AgentCommand>, String> {
        self.conversation
            .pending_commands()
            .map_err(|error| error.to_string())
    }

    async fn start(&mut self, input: UserInput) -> Result<AgentRun, String> {
        let model = match &self.config.model {
            Some(model) => model.clone(),
            None => {
                let model = self
                    .provider
                    .models()
                    .await
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .next()
                    .ok_or("No models available; log in with antex login.")?
                    .id;
                self.config.model = Some(model.clone());
                model
            }
        };
        if self.agent.is_none() {
            let mut runtime = LocalRuntime::new(&self.workspace, self.config.permissions)
                .map_err(|error| error.to_string())?
                .with_read_roots(&self.context.read_roots)
                .map_err(|error| error.to_string())?
                .with_shell_timeout(std::time::Duration::from_secs(
                    self.config.shell_timeout_seconds,
                ));
            if let Some(program) = &self.bubblewrap {
                runtime = runtime.with_bubblewrap(program.clone());
            }
            let compaction = Arc::new(Compaction::new(
                self.provider.clone(),
                self.context.clone(),
                model.clone(),
                self.config.context_token_limit,
            ));
            self.agent = Some(
                Agent::new(self.provider.clone(), Arc::new(runtime))
                    .with_context_hook(compaction.clone()),
            );
            self.compaction = Some(compaction);
        }
        Ok(self
            .agent
            .as_mut()
            .ok_or("Agent is unavailable")?
            .start(TurnInput {
                model,
                reasoning: self.config.model_reasoning_effort.clone(),
                history: self.conversation.messages(),
                input,
            }))
    }

    fn record(&mut self, event: &AgentEvent) -> Result<(), String> {
        self.conversation
            .record(event)
            .map_err(|error| error.to_string())
    }

    async fn command(&mut self, command: &str) -> Result<CommandEffect, String> {
        let (name, argument) = command
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((command.trim(), ""));
        let argument = argument.trim();
        match name {
            "/help" => Ok(CommandEffect::Notice("/model [id], /cd <path>, /sessions, /resume <id>, /fork <record-id>, /compact, /status, /quit".into())),
            "/model" => {
                let models = self.provider.models().await.map_err(|error| error.to_string())?;
                if argument.is_empty() { return Ok(CommandEffect::Notice(models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>().join(", "))); }
                if !models.iter().any(|model| model.id == argument) { return Err("Choose a model listed by /model.".into()); }
                self.config.model = Some(argument.into());
                self.reset_agent();
                Ok(CommandEffect::Notice(format!("Model: {argument}")))
            }
            "/cd" => {
                if argument.is_empty() { return Err("Usage: /cd <directory>".into()); }
                let workspace = self.workspace.join(argument).canonicalize().map_err(|error| error.to_string())?;
                let context = ProjectContext::load(&self.home, &workspace).map_err(|error| error.to_string())?;
                let store = SessionStore::new(&self.home, &workspace).map_err(|error| error.to_string())?;
                let conversation = Conversation::new(store.create().map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
                self.workspace = workspace;
                self.context = context;
                self.conversation = conversation;
                self.reset_agent();
                Ok(CommandEffect::Reset(Vec::new()))
            }
            "/sessions" => {
                let store = SessionStore::new(&self.home, &self.workspace).map_err(|error| error.to_string())?;
                Ok(CommandEffect::Notice(store.list().map_err(|error| error.to_string())?.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")))
            }
            "/resume" => {
                let id = argument.parse().map_err(|_| "Usage: /resume <session-id>")?;
                let store = SessionStore::new(&self.home, &self.workspace).map_err(|error| error.to_string())?;
                self.conversation = Conversation::new(store.open(id).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
                self.reset_agent();
                Ok(CommandEffect::Reset(self.history()))
            }
            "/fork" => {
                let parent = argument.parse().map_err(|_| "Usage: /fork <record-id>")?;
                self.conversation.branch(parent).map_err(|error| error.to_string())?;
                self.reset_agent();
                Ok(CommandEffect::Reset(self.history()))
            }
            "/compact" => {
                let compaction = self.compaction.as_ref().ok_or("Start a conversation before compacting.")?;
                let checkpoint = compaction.compact(&self.history()).await.map_err(|error| error.to_string())?;
                self.record(&AgentEvent::ContextCheckpoint(checkpoint))?;
                Ok(CommandEffect::Notice("Context compacted; original records retained.".into()))
            }
            "/status" => Ok(CommandEffect::Notice(format!("Session {} · {} records", self.conversation.id(), self.conversation.entries().len()))),
            _ => Err("Unknown command. Use /help.".into()),
        }
    }
}
