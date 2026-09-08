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
use antex_extension_protocol::WorkspaceAccess;
use antex_runtime::ExtensionSandbox;
use antex_runtime::ExtensionWorkspace;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use super::*;

pub(super) struct ActionExecutor {
    pub config: Config,
    pub context: ProjectContext,
    pub provider: Arc<OpenAiProvider>,
    pub tools: Arc<dyn ToolHost>,
    pub sandbox: ExtensionSandbox,
    pub history: Vec<Message>,
    pub transcript: String,
}

impl ActionExecutor {
    pub async fn execute(&mut self, actions: Vec<Action>) -> Vec<ActionResult> {
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
                let _ = text;
                let result = Err("terminal logging requires the interactive session".into());
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

    async fn run_extension_shell(
        &self,
        script: &str,
        workspace: WorkspaceAccess,
        network: NetworkAccess,
    ) -> Result<serde_json::Value, String> {
        let workspace = match workspace {
            WorkspaceAccess::ReadOnly => ExtensionWorkspace::ReadOnly,
            WorkspaceAccess::ReadWrite => ExtensionWorkspace::ReadWrite,
        };
        let result = self
            .sandbox
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
            Inspection::Transcript => self.transcript.clone(),
            Inspection::Context | Inspection::SystemPrompt => {
                let prepared = self
                    .context
                    .prepare(&self.history)
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
        let tools = self.tools.clone();
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
#[path = "action_executor_tests.rs"]
mod tests;
