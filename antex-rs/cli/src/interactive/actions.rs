use std::time::Duration;

use antex_core::AgentCommand;
use antex_core::Content;
use antex_core::ContextHook;
use antex_core::ContextKind;
use antex_core::InteractionAnswer;
use antex_core::Message;
use antex_core::ToolHost;
use antex_core::TurnInput;
use antex_extension_protocol::Action;
use antex_extension_protocol::ActionResult;
use antex_extension_protocol::Inspection;
use antex_extension_protocol::NetworkAccess;
use antex_extension_protocol::Output;
use antex_extension_protocol::WorkspaceAccess;
use antex_runtime::ExtensionSandbox;
use antex_runtime::ExtensionWorkspace;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use super::*;

impl InteractiveSession {
    pub(super) async fn apply_extension_output(
        &mut self,
        extension: &str,
        output: Output,
    ) -> Result<CommandEffect, String> {
        let mut rendered = Vec::new();
        let mut text = String::new();
        let mut panel = None;
        let mut pending = vec![output];
        let mut action_count = 0usize;
        while let Some(output) = pending.pop() {
            for record in output.records {
                self.conversation
                    .append_extension(extension, record)
                    .map_err(|error| error.to_string())?;
            }
            if let Some(status) = output.status {
                text = status;
            } else if !output.text.is_empty() {
                text = output.text;
            }
            panel = output.panel.or(panel);
            action_count += output.actions.len();
            if action_count > 64 {
                return Err("extension command exceeds its 64-action budget".into());
            }
            for result in self.execute_extension_actions(output.actions).await {
                if self.extensions.is_none()
                    && let Some(result_text) = result.data["text"].as_str()
                    && !result_text.is_empty()
                {
                    rendered.push(result_text.to_owned());
                }
                self.conversation
                    .append_extension(
                        extension,
                        serde_json::json!({"actionResult":result.clone()}),
                    )
                    .map_err(|error| error.to_string())?;
                if let Some(extensions) = &self.extensions
                    && let Some(next) = extensions
                        .continue_action(extension, result)
                        .await
                        .map_err(|error| error.to_string())?
                {
                    pending.push(next);
                }
            }
        }
        if let Some(panel) = panel {
            return Ok(CommandEffect::Page(antex_tui::TextPage {
                title: panel.title,
                body: panel.text,
                older_command: None,
            }));
        }
        if !rendered.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&rendered.join("\n\n"));
        }
        if text.is_empty() {
            text = "Extension command completed.".into();
        }
        Ok(CommandEffect::Notice(text))
    }

    async fn execute_extension_actions(&mut self, actions: Vec<Action>) -> Vec<ActionResult> {
        if actions.len() > 1
            && actions
                .iter()
                .all(|action| matches!(action, Action::Agent { .. }))
        {
            let ids = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Agent { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            return match self.run_extension_agent_batch(actions).await {
                Ok(results) => results,
                Err(error) => ids
                    .into_iter()
                    .map(|id| ActionResult {
                        id,
                        succeeded: false,
                        data: serde_json::json!({"text":error}),
                    })
                    .collect(),
            };
        }
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            results.push(self.execute_extension_action(action).await);
        }
        results
    }

    async fn execute_extension_action(&mut self, action: Action) -> ActionResult {
        let (id, result) = match action {
            Action::TerminalLog { id, text } => {
                let result = self
                    .append_terminal_log(&text)
                    .map(|_| serde_json::json!({"text":""}));
                (id, result)
            }
            Action::Inspect { id, target } => (id, self.inspect(target).await),
            Action::Shell {
                id,
                command,
                workspace,
                network,
            } => (
                id,
                self.run_extension_shell(&command, workspace, network).await,
            ),
            Action::Agent { id, prompt, model } => {
                let result = self
                    .run_extension_agent_batch(vec![Action::Agent {
                        id: id.clone(),
                        prompt,
                        model,
                    }])
                    .await
                    .and_then(|mut results| {
                        results
                            .pop()
                            .ok_or_else(|| "agent returned no result".to_string())
                    });
                return result.unwrap_or_else(|error| ActionResult {
                    id,
                    succeeded: false,
                    data: serde_json::json!({"text":error}),
                });
            }
        };
        match result {
            Ok(data) => ActionResult {
                id,
                succeeded: true,
                data,
            },
            Err(error) => ActionResult {
                id,
                succeeded: false,
                data: serde_json::json!({"text":error}),
            },
        }
    }

    pub(super) fn append_terminal_log(&mut self, text: &str) -> Result<(), String> {
        if self.terminal_log.is_none() {
            self.terminal_log = Some(
                TmuxLog::start(&self.home, self.conversation.id())
                    .map_err(|error| error.to_string())?,
            );
        }
        self.terminal_log
            .as_mut()
            .ok_or_else(|| "tmux log is unavailable".to_string())?
            .append(text)
            .map_err(|error| error.to_string())
    }

    async fn run_extension_shell(
        &self,
        script: &str,
        workspace: WorkspaceAccess,
        network: NetworkAccess,
    ) -> Result<serde_json::Value, String> {
        let sandbox = ExtensionSandbox::new(
            &self.home,
            &self.workspace,
            self.bubblewrap
                .clone()
                .unwrap_or_else(|| "/usr/bin/bwrap".into()),
            &self.context.read_roots,
        )
        .map_err(|error| error.to_string())?;
        let workspace = match workspace {
            WorkspaceAccess::ReadOnly => ExtensionWorkspace::ReadOnly,
            WorkspaceAccess::ReadWrite => ExtensionWorkspace::ReadWrite,
        };
        let result = sandbox
            .run_shell(
                script,
                workspace,
                network == NetworkAccess::Allowed,
                Duration::from_secs(self.config.shell_timeout_seconds),
                CancellationToken::new(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let text = format!(
            "{}{}{}",
            result.stdout,
            if !result.stdout.is_empty() && !result.stderr.is_empty() {
                "\n"
            } else {
                ""
            },
            result.stderr
        );
        Ok(serde_json::json!({"text":text,"exitCode":result.exit_code}))
    }

    async fn inspect(&mut self, target: Inspection) -> Result<serde_json::Value, String> {
        let text = match target {
            Inspection::Transcript => {
                self.conversation
                    .transcript_page(/*cursor*/ None)
                    .map_err(|error| error.to_string())?
                    .text
            }
            Inspection::Context | Inspection::SystemPrompt => {
                let prepared = self
                    .context
                    .prepare(&self.history())
                    .await
                    .map_err(|error| error.to_string())?;
                prepared
                    .messages
                    .into_iter()
                    .filter_map(|message| match message {
                        Message::Context(fragment)
                            if target == Inspection::Context
                                || fragment.kind() == ContextKind::System =>
                        {
                            Some(fragment.text().to_owned())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
        };
        Ok(serde_json::json!({"text":bounded(&text)}))
    }

    async fn run_extension_agent_batch(
        &mut self,
        actions: Vec<Action>,
    ) -> Result<Vec<ActionResult>, String> {
        let needs_model = actions
            .iter()
            .any(|action| matches!(action, Action::Agent { model: None, .. }));
        let default_model = match self.config.model.clone() {
            Some(model) => Some(model),
            None if needs_model => Some(
                self.provider
                    .models()
                    .await
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .next()
                    .ok_or_else(|| "no model is available".to_string())?
                    .id,
            ),
            None => None,
        };
        let tools = self.tools().await?;
        Ok(run_agents(
            self.provider.clone(),
            tools,
            Arc::new(self.context.clone()),
            self.config.model_reasoning_effort.clone(),
            default_model,
            actions,
        )
        .await)
    }
}

async fn run_agents<P: antex_core::ModelProvider + 'static>(
    provider: Arc<P>,
    tools: Arc<dyn ToolHost>,
    context: Arc<dyn ContextHook>,
    reasoning: Option<String>,
    default_model: Option<String>,
    actions: Vec<Action>,
) -> Vec<ActionResult> {
    futures::stream::iter(actions)
        .map(|action| {
            let provider = provider.clone();
            let tools = tools.clone();
            let context = Arc::clone(&context);
            let reasoning = reasoning.clone();
            let default_model = default_model.clone();
            async move {
                let Action::Agent { id, prompt, model } = action else {
                    unreachable!()
                };
                let result = run_agent(
                    provider,
                    tools,
                    context,
                    reasoning,
                    model.or(default_model).expect("resolved model"),
                    prompt,
                )
                .await;
                match result {
                    Ok(data) => ActionResult {
                        id,
                        succeeded: true,
                        data,
                    },
                    Err(error) => ActionResult {
                        id,
                        succeeded: false,
                        data: serde_json::json!({"text":error}),
                    },
                }
            }
        })
        .buffered(8)
        .collect()
        .await
}

async fn run_agent<P: antex_core::ModelProvider + 'static>(
    provider: Arc<P>,
    tools: Arc<dyn ToolHost>,
    context: Arc<dyn ContextHook>,
    reasoning: Option<String>,
    model: String,
    prompt: String,
) -> Result<serde_json::Value, String> {
    let mut agent = Agent::new(provider, tools).with_context_hook(context);
    let mut run = agent.start(TurnInput {
        model,
        reasoning,
        history: Vec::new(),
        input: prompt.as_str().into(),
    });
    let mut answer = String::new();
    let result = tokio::time::timeout(Duration::from_secs(900), async {
        while let Some(event) = run.events.recv().await {
            match event {
                AgentEvent::MessageCommitted(Message::Assistant { content, .. }) => {
                    for block in content {
                        if let Content::Text(text) = block {
                            answer.push_str(&text);
                        }
                    }
                }
                AgentEvent::Interaction { request, .. } => {
                    request
                        .answer(InteractionAnswer::Deny)
                        .map_err(|error| error.to_string())?;
                }
                AgentEvent::Error(error) => return Err(error.to_string()),
                AgentEvent::Finished { reason, .. } => {
                    return match reason {
                        antex_core::FinishReason::Completed => Ok(()),
                        antex_core::FinishReason::Interrupted => Err("agent interrupted".into()),
                        antex_core::FinishReason::Failed => Err("agent failed".into()),
                    };
                }
                _ => {}
            }
        }
        Err("agent event stream ended".into())
    })
    .await
    .map_err(|_| {
        let _ = run.commands.try_send(AgentCommand::Interrupt);
        "agent action timed out".to_string()
    })?;
    result?;
    Ok(serde_json::json!({"text":bounded(&answer)}))
}

fn bounded(text: &str) -> String {
    let mut end = text.len().min(antex_core::MAX_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
