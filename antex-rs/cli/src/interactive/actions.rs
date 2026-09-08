use antex_extension_protocol::Action;
use antex_extension_protocol::ActionResult;
use antex_extension_protocol::Output;
use antex_runtime::ExtensionSandbox;

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
                    .append_extension_event(
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
        self.conversation
            .flush()
            .map_err(|error| error.to_string())?;
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
        let mut executor = match self.action_executor() {
            Ok(executor) => executor,
            Err(error) => {
                return actions
                    .into_iter()
                    .map(|action| {
                        let id = match action {
                            Action::Agent { id, .. }
                            | Action::Shell { id, .. }
                            | Action::Inspect { id, .. }
                            | Action::TerminalLog { id, .. } => id,
                        };
                        ActionResult {
                            id,
                            succeeded: false,
                            data: serde_json::json!({"text":error}),
                        }
                    })
                    .collect();
            }
        };
        if actions
            .iter()
            .all(|action| matches!(action, Action::Agent { .. }))
        {
            return executor.execute(actions).await;
        }
        let mut results = Vec::new();
        for action in actions {
            if let Action::TerminalLog { id, text } = action {
                let result = self.append_terminal_log(&text);
                results.push(ActionResult {
                    id,
                    succeeded: result.is_ok(),
                    data: serde_json::json!({"text":result.err().unwrap_or_default()}),
                });
            } else {
                results.extend(executor.execute(vec![action]).await);
            }
        }
        results
    }

    pub(super) fn action_executor(
        &mut self,
    ) -> Result<super::action_executor::ActionExecutor, String> {
        let tools = match &self.extensions {
            Some(extensions) => extensions.tool_host(),
            None => self.local_runtime()?,
        };
        Ok(super::action_executor::ActionExecutor {
            config: self.config.clone(),
            context: self.context.clone(),
            provider: self.provider.clone(),
            tools,
            sandbox: ExtensionSandbox::new(
                &self.home,
                &self.workspace,
                self.bubblewrap
                    .clone()
                    .unwrap_or_else(|| "/usr/bin/bwrap".into()),
                &self.context.read_roots,
            )
            .map_err(|error| error.to_string())?,
            history: self.history(),
            transcript: self
                .conversation
                .transcript_page(None)
                .map_err(|error| error.to_string())?
                .text,
        })
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
}
