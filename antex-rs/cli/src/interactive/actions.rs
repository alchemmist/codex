use std::time::Duration;

use antex_core::AgentCommand;
use antex_core::Content;
use antex_core::ContextHook;
use antex_core::ContextKind;
use antex_core::InteractionAnswer;
use antex_core::Message;
use antex_core::TurnInput;
use antex_extension_protocol::Action;
use antex_extension_protocol::ActionResult;
use antex_extension_protocol::Inspection;
use antex_extension_protocol::NetworkAccess;
use antex_extension_protocol::Output;
use antex_extension_protocol::WorkspaceAccess;
use antex_runtime::ExtensionSandbox;
use antex_runtime::ExtensionWorkspace;
use tokio_util::sync::CancellationToken;

use super::*;

impl InteractiveSession {
    pub(super) async fn apply_extension_output(
        &mut self,
        extension: &str,
        output: Output,
    ) -> Result<CommandEffect, String> {
        for record in output.records {
            self.conversation
                .append_extension(extension, record)
                .map_err(|error| error.to_string())?;
        }
        let mut rendered = Vec::new();
        for action in output.actions {
            let result = self.execute_extension_action(action).await;
            if let Some(text) = result.data["text"].as_str()
                && !text.is_empty()
            {
                rendered.push(text.to_owned());
            }
            self.conversation
                .append_extension(extension, serde_json::json!({"actionResult":result}))
                .map_err(|error| error.to_string())?;
        }
        if let Some(panel) = output.panel {
            return Ok(CommandEffect::Page(antex_tui::TextPage {
                title: panel.title,
                body: panel.text,
                older_command: None,
            }));
        }
        let mut text = output.status.unwrap_or(output.text);
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
                (id, self.run_extension_agent(prompt, model).await)
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

    async fn run_extension_agent(
        &mut self,
        prompt: String,
        model: Option<String>,
    ) -> Result<serde_json::Value, String> {
        let model = match model.or_else(|| self.config.model.clone()) {
            Some(model) => model,
            None => {
                self.provider
                    .models()
                    .await
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .next()
                    .ok_or_else(|| "no model is available".to_string())?
                    .id
            }
        };
        let tools = self.tools().await?;
        let mut agent = Agent::new(self.provider.clone(), tools)
            .with_context_hook(Arc::new(self.context.clone()));
        let mut run = agent.start(TurnInput {
            model,
            reasoning: self.config.model_reasoning_effort.clone(),
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
                            antex_core::FinishReason::Interrupted => {
                                Err("agent interrupted".into())
                            }
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
}

fn bounded(text: &str) -> String {
    let mut end = text.len().min(antex_core::MAX_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
