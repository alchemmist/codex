use std::path::PathBuf;
use std::sync::Arc;

use antex_core::Agent;
use antex_core::AgentEvent;
use antex_core::AgentRun;
use antex_core::Message;
use antex_core::ModelProvider;
use antex_core::ToolHost;
use antex_core::TurnInput;
use antex_core::UserInput;
use antex_extension_host::ExtensionRegistry;
use antex_extension_host::ExtensionRegistryConfig;
use antex_provider_openai::OpenAiProvider;
use antex_runtime::Compaction;
use antex_runtime::Config;
use antex_runtime::Conversation;
use antex_runtime::ExtensionSandbox;
use antex_runtime::LocalRuntime;
use antex_runtime::ProjectContext;
use antex_runtime::SessionStore;
use antex_tui::CommandEffect;
use antex_tui::Session;
use antex_tui::SessionView;

use crate::extension_launcher::RuntimeExtensionLauncher;
use crate::terminal_log::TmuxLog;

#[path = "interactive/actions.rs"]
mod actions;

#[path = "interactive/action_executor.rs"]
mod action_executor;

#[path = "interactive/workflow.rs"]
mod workflow;

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
    extensions: Option<Arc<ExtensionRegistry>>,
    workflow: Option<workflow::Workflow>,
    terminal_log: Option<TmuxLog>,
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
            extensions: None,
            workflow: None,
            terminal_log: None,
        })
    }

    fn reset_agent(&mut self) {
        self.agent = None;
        self.compaction = None;
        self.extensions = None;
    }

    fn local_runtime(&self) -> Result<Arc<dyn ToolHost>, String> {
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
        Ok(Arc::new(runtime))
    }

    async fn tools(&mut self) -> Result<Arc<dyn ToolHost>, String> {
        let runtime = self.local_runtime()?;
        if !self.config.extensions {
            return Ok(runtime);
        }
        if self.extensions.is_none() {
            let states = self
                .conversation
                .extension_states()
                .map_err(|error| error.to_string())?;
            let workflows = self.home.join("workflows");
            let mut read_roots = self.context.read_roots.clone();
            if workflows.is_dir() && !workflows.is_symlink() {
                read_roots.push(workflows.clone());
            }
            let sandbox = ExtensionSandbox::new(
                &self.home,
                &self.workspace,
                self.bubblewrap
                    .clone()
                    .unwrap_or_else(|| "/usr/bin/bwrap".into()),
                &read_roots,
            )
            .map_err(|error| error.to_string())?;
            let session_id = self.conversation.id().to_string();
            let loaded = ExtensionRegistry::load(ExtensionRegistryConfig {
                home: &self.home,
                workspace: &self.workspace,
                project_trusted: self.config.trust_project_extensions,
                fallback: runtime,
                antex_version: env!("ANTEX_BUILD_VERSION"),
                session_id: &session_id,
                launcher: Arc::new(RuntimeExtensionLauncher::new(
                    sandbox,
                    workflows,
                    self.config.trust_project_extensions,
                )),
                states: &states,
            })
            .await
            .map_err(|error| error.to_string())?;
            for failure in loaded.failures {
                eprintln!("antex: extension failed: {failure}");
            }
            self.extensions = Some(Arc::new(loaded.registry));
        }
        Ok(self
            .extensions
            .as_ref()
            .ok_or("Extension registry is unavailable")?
            .tool_host())
    }
}

impl Session for InteractiveSession {
    async fn background_notice(&mut self) -> Result<String, String> {
        self.next_workflow_update().await
    }
    fn queue_command(&mut self, command: &antex_core::AgentCommand) -> Result<(), String> {
        self.conversation
            .queue_command(command)
            .map_err(|error| error.to_string())
    }
    async fn prepare_image(
        &mut self,
        source: antex_tui::ImageSource,
    ) -> Result<antex_core::Content, String> {
        let image = match source {
            antex_tui::ImageSource::File(path) => {
                antex_runtime::ImageAttachment::load(&self.workspace.join(path))
            }
            antex_tui::ImageSource::Rgba {
                width,
                height,
                bytes,
            } => antex_runtime::ImageAttachment::from_rgba(width, height, bytes),
        }
        .map_err(|error| error.to_string())?;
        Ok(image.content)
    }
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
            let tools = self.tools().await?;
            let compaction = Arc::new(Compaction::new(
                self.provider.clone(),
                self.context.clone(),
                model.clone(),
                self.config.context_token_limit,
            ));
            self.agent = Some(
                Agent::new(self.provider.clone(), tools).with_context_hook(compaction.clone()),
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

    async fn record(&mut self, event: &AgentEvent) -> Result<(), String> {
        self.conversation
            .record(event)
            .map_err(|error| error.to_string())?;
        let Some(event) = extension_event(event) else {
            return Ok(());
        };
        if let Some(extensions) = &self.extensions {
            let delivery = extensions.notify(event).await;
            for failure in delivery.failures {
                eprintln!("antex: extension event failed: {failure}");
            }
            for event in delivery.outputs {
                for record in event.output.records {
                    self.conversation
                        .append_extension(&event.extension, record)
                        .map_err(|error| error.to_string())?;
                }
                for action in event.output.actions {
                    let antex_extension_protocol::Action::TerminalLog { text, .. } = action else {
                        continue;
                    };
                    if let Err(error) = self.append_terminal_log(&text) {
                        eprintln!("antex: tmux command log failed: {error}");
                    }
                }
            }
        }
        Ok(())
    }

    async fn command(&mut self, command: &str) -> Result<CommandEffect, String> {
        let (name, argument) = command
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((command.trim(), ""));
        let argument = argument.trim();
        if self.workflow.is_some() && matches!(name, "/cd" | "/resume" | "/fork" | "/model") {
            return Err("Stop the workflow before changing session, directory or model.".into());
        }
        if name == "/workflow"
            && let Some(effect) = self.workflow_control(argument).await?
        {
            return Ok(effect);
        }
        let name = if name == "/resume" && argument.is_empty() {
            "/sessions"
        } else {
            name
        };
        match name {
            "/image" => {
                if argument.is_empty() { return Err("Usage: /image <path>".into()); }
                let path = antex_tui::parse_image_path(argument).ok_or("Usage: /image <path>; quote paths containing spaces.")?;
                let image = self.prepare_image(antex_tui::ImageSource::File(path)).await?;
                Ok(CommandEffect::Image(image))
            }
            "/help" => Ok(CommandEffect::Page(antex_tui::TextPage { title: "Antex commands".into(), body: "# Conversation\n\n- `/model` — choose a model\n- `/sessions` or `/resume` — choose a session\n- `/fork` — branch from a user message\n- `/cd <path>` — change workspace\n- `/compact` — summarize active context\n- `/transcript` — inspect original messages, tools, and reasoning summaries\n- `/extensions` — list extension commands\n- `/status` — show session identity\n\n# Input and appearance\n\n- Ctrl+S — stash or append the saved draft\n- Ctrl+V or `/image <path>` — attach an image\n- Ctrl+O or `/copy` — copy the last response\n- Ctrl+T — open the transcript\n- Ctrl+L — clear the visible screen\n- `/theme` — choose a syntax theme\n- Enter — send or steer; Tab — queue a follow-up\n- Ctrl+C — interrupt; `/quit` — exit\n".into(), older_command: None })),
            "/transcript" => {
                let cursor = if argument.is_empty() { None } else { Some(argument.parse().map_err(|_| "Invalid transcript cursor.")?) };
                let page = self.conversation.transcript_page(cursor).map_err(|error| error.to_string())?;
                Ok(CommandEffect::Page(antex_tui::TextPage { title: format!("Transcript · {}", self.conversation.id()), body: page.text, older_command: page.next_cursor.map(|cursor| format!("/transcript {cursor}")) }))
            }
            "/model" => {
                let models = self.provider.models().await.map_err(|error| error.to_string())?;
                if argument.is_empty() {
                    return Ok(CommandEffect::Picker(antex_tui::PickerSpec { title: "Choose a model".into(), items: models.into_iter().take(256).map(|model| antex_tui::PickerItem { label: model.display_name, description: format!("{} · {} token context", model.id, model.context_window), command: format!("/model {}", model.id) }).collect() }));
                }
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
                let previews = tokio::task::spawn_blocking(move || store.previews(/*limit*/ 256)).await.map_err(|error| error.to_string())?.map_err(|error| error.to_string())?;
                Ok(CommandEffect::Picker(antex_tui::PickerSpec { title: "Recent sessions".into(), items: previews.into_iter().filter(|preview| preview.id != self.conversation.id()).map(|preview| antex_tui::PickerItem { label: preview.text, description: preview.id.to_string(), command: format!("/resume {}", preview.id) }).collect() }))
            }
            "/resume" => {
                let id = argument.parse().map_err(|_| "Usage: /resume <session-id>")?;
                if id == self.conversation.id() { return Ok(CommandEffect::Notice("This session is already active.".into())); }
                let store = SessionStore::new(&self.home, &self.workspace).map_err(|error| error.to_string())?;
                self.conversation = Conversation::new(store.open(id).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
                self.reset_agent();
                Ok(CommandEffect::Reset(self.history()))
            }
            "/fork" => {
                if argument.is_empty() {
                    return Ok(CommandEffect::Picker(antex_tui::PickerSpec { title: "Fork from a user message".into(), items: self.conversation.entries().iter().rev().filter_map(|entry| {
                        let Message::User(input) = &entry.message else { return None; };
                        let label = input.content.iter().filter_map(|content| match content { antex_core::Content::Text(text) => Some(text.as_str()), _ => None }).flat_map(str::chars).take(120).collect::<String>();
                        Some(antex_tui::PickerItem { label: if label.is_empty() { "[Image message]".into() } else { label }, description: entry.record_id.to_string(), command: format!("/fork {}", entry.record_id) })
                    }).take(256).collect() }));
                }
                let parent = argument.parse().map_err(|_| "Usage: /fork <record-id>")?;
                self.conversation.branch(parent).map_err(|error| error.to_string())?;
                self.reset_agent();
                Ok(CommandEffect::Reset(self.history()))
            }
            "/compact" => {
                let compaction = self.compaction.as_ref().ok_or("Start a conversation before compacting.")?;
                let checkpoint = compaction.compact(&self.history()).await.map_err(|error| error.to_string())?;
                self.record(&AgentEvent::ContextCheckpoint(checkpoint)).await?;
                Ok(CommandEffect::Notice("Context compacted; original records retained.".into()))
            }
            "/status" => Ok(CommandEffect::Notice(format!("Session {} · {} records", self.conversation.id(), self.conversation.entries().len()))),
            "/extensions" => {
                self.tools().await?;
                let body = self.extensions.as_ref().map(|extensions| extensions.commands().iter().map(|command| format!("- `/{} {}` — {}", command.name, "[arguments]", command.description)).collect::<Vec<_>>().join("\n")).filter(|body| !body.is_empty()).unwrap_or_else(|| "No extension commands enabled.".into());
                Ok(CommandEffect::Page(antex_tui::TextPage { title: "Extensions".into(), body, older_command: None }))
            }
            _ => {
                self.tools().await?;
                let extension_name = name.strip_prefix('/').unwrap_or(name);
                let Some(extensions) = self.extensions.as_ref() else { return Err("Unknown command. Use /help.".into()); };
                if !extensions.commands().iter().any(|command| command.name == extension_name) { return Err("Unknown command. Use /help.".into()); }
                if extension_name == "workflow" && !argument.is_empty() && !matches!(argument, "list" | "status" | "pause" | "stop") {
                    return self.start_workflow(argument);
                }
                if extension_name == "workflow" && argument.is_empty() {
                    let output = extensions.run_command(extension_name, "list".into(), tokio_util::sync::CancellationToken::new()).await.map_err(|error| error.to_string())?;
                    let items = output.output.text.lines().filter(|id| !id.is_empty() && id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))).take(256).map(|id| antex_tui::PickerItem { label: id.into(), description: "Run workflow".into(), command: format!("/workflow {id}") }).collect();
                    return Ok(CommandEffect::Picker(antex_tui::PickerSpec { title: "Choose a workflow".into(), items }));
                }
                let output = extensions.run_command(extension_name, argument.into(), tokio_util::sync::CancellationToken::new()).await.map_err(|error| error.to_string())?;
                self.apply_extension_output(&output.extension, output.output).await
            }
        }
    }
}

fn extension_event(event: &AgentEvent) -> Option<antex_extension_protocol::Event> {
    let (name, data) = match event {
        AgentEvent::ToolStarted(call) => (
            "toolStarted",
            serde_json::json!({"callId":call.id,"name":call.name,"arguments":call.arguments}),
        ),
        AgentEvent::MessageCommitted(Message::Tool(output)) => (
            "toolCompleted",
            serde_json::json!({"callId":output.call_id,"outcome":format!("{:?}",output.outcome).to_lowercase(),"text":output.text()}),
        ),
        AgentEvent::TurnCompleted => ("turnComplete", serde_json::json!({})),
        _ => return None,
    };
    Some(antex_extension_protocol::Event {
        name: name.into(),
        data,
    })
}

#[cfg(test)]
#[path = "interactive_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "interactive/workflow_tests.rs"]
mod workflow_tests;
