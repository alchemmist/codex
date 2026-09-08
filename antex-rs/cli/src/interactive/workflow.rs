use antex_extension_protocol::{Action, ActionResult, Output};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use super::*;

pub(super) enum Update {
    Output(Output, oneshot::Sender<()>),
    Action(ActionResult, oneshot::Sender<()>),
    Failure(String),
}

pub(super) struct Workflow {
    updates: mpsc::Receiver<Update>,
    paused: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl Drop for Workflow {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl InteractiveSession {
    pub(super) fn start_workflow(&mut self, arguments: &str) -> Result<CommandEffect, String> {
        if self.workflow.is_some() {
            return Err("A workflow is active; use /workflow pause, resume or stop.".into());
        }
        let executor = self.action_executor()?;
        let registry = self
            .extensions
            .clone()
            .ok_or("Extensions are unavailable")?;
        let (sender, updates) = mpsc::channel(4);
        let (paused, control) = watch::channel(false);
        let arguments = arguments.to_owned();
        let task = tokio::spawn(async move {
            if let Err(error) = drive(registry, executor, arguments, &sender, control).await {
                let _ = sender.send(Update::Failure(error)).await;
            }
        });
        self.workflow = Some(Workflow {
            updates,
            paused,
            task,
        });
        Ok(CommandEffect::Notice("Workflow started.".into()))
    }

    pub(super) async fn workflow_control(
        &mut self,
        command: &str,
    ) -> Result<Option<CommandEffect>, String> {
        let Some(workflow) = self.workflow.as_mut() else {
            return Ok(None);
        };
        let text = match command {
            "pause" => {
                workflow.paused.send_replace(true);
                "Workflow paused; the current action may finish."
            }
            "resume" => {
                workflow.paused.send_replace(false);
                "Workflow resumed."
            }
            "status" => {
                if *workflow.paused.borrow() {
                    "Workflow paused."
                } else {
                    "Workflow running."
                }
            }
            "stop" => {
                self.stop_workflow("stopped").await?;
                "Workflow stopped."
            }
            _ => return Err("A workflow is active; use /workflow pause, resume or stop.".into()),
        };
        Ok(Some(CommandEffect::Notice(text.into())))
    }

    async fn stop_workflow(&mut self, phase: &str) -> Result<(), String> {
        if let Some(mut workflow) = self.workflow.take() {
            workflow.task.abort();
            let _ = (&mut workflow.task).await;
        }
        let state = self
            .conversation
            .extension_states()
            .map_err(|error| error.to_string())?
            .remove("workflows");
        let mut state = state.unwrap_or(serde_json::Value::Null);
        if state.is_object() && state["phase"] != "completed" && state["phase"] != "failed" {
            state["phase"] = phase.into();
            self.conversation
                .append_extension("workflows", state.clone())
                .map_err(|error| error.to_string())?;
            self.conversation
                .flush()
                .map_err(|error| error.to_string())?;
        }
        if let Some(extensions) = &self.extensions {
            extensions
                .reset("workflows", state)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) async fn next_workflow_update(&mut self) -> Result<String, String> {
        let Some(workflow) = self.workflow.as_mut() else {
            return std::future::pending().await;
        };
        let update = workflow.updates.recv().await;
        match update {
            Some(Update::Output(output, acknowledged)) => {
                for record in output.records {
                    self.conversation
                        .append_extension("workflows", record)
                        .map_err(|error| error.to_string())?;
                }
                self.conversation
                    .flush()
                    .map_err(|error| error.to_string())?;
                let text = output.status.unwrap_or(output.text);
                let _ = acknowledged.send(());
                Ok(text)
            }
            Some(Update::Action(result, acknowledged)) => {
                self.conversation
                    .append_extension_event("workflows", serde_json::json!({"actionResult":result}))
                    .map_err(|error| error.to_string())?;
                let _ = acknowledged.send(());
                Ok(String::new())
            }
            Some(Update::Failure(error)) => {
                self.stop_workflow("failed").await?;
                Err(error)
            }
            None => {
                self.workflow = None;
                Ok(String::new())
            }
        }
    }
}

async fn deliver(sender: &mpsc::Sender<Update>, output: Output) -> Result<(), String> {
    let (acknowledged, waiting) = oneshot::channel();
    sender
        .send(Update::Output(output, acknowledged))
        .await
        .map_err(|_| "workflow session closed")?;
    waiting
        .await
        .map_err(|_| "workflow output was not persisted".into())
}

async fn drive(
    registry: Arc<ExtensionRegistry>,
    mut executor: super::action_executor::ActionExecutor,
    arguments: String,
    sender: &mpsc::Sender<Update>,
    mut paused: watch::Receiver<bool>,
) -> Result<(), String> {
    let output = registry
        .run_command(
            "workflow",
            arguments,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .map_err(|error| error.to_string())?;
    if output.extension != "workflows" {
        return Err("Workflow command belongs to another extension".into());
    }
    let mut pending = vec![output.output];
    let mut active = true;
    let mut count = 0usize;
    loop {
        while *paused.borrow_and_update() {
            paused
                .changed()
                .await
                .map_err(|_| "workflow control closed")?;
        }
        let output = match pending.pop() {
            Some(output) => output,
            None if active => {
                registry
                    .run_command(
                        "workflow",
                        "poll".into(),
                        tokio_util::sync::CancellationToken::new(),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .output
            }
            None => return Ok(()),
        };
        if let Some(record) = output.records.last() {
            active = record["phase"] == "running";
        }
        let actions = output.actions.clone();
        count += actions.len();
        if count > 64 {
            return Err("Workflow exceeds its 64-action budget".into());
        }
        if actions
            .iter()
            .any(|action| matches!(action, Action::TerminalLog { .. }))
        {
            return Err("Use ctx.log for workflow logging".into());
        }
        deliver(sender, output).await?;
        while *paused.borrow_and_update() {
            paused
                .changed()
                .await
                .map_err(|_| "workflow control closed")?;
        }
        for result in executor.execute(actions).await {
            let (acknowledged, waiting) = oneshot::channel();
            sender
                .send(Update::Action(result.clone(), acknowledged))
                .await
                .map_err(|_| "workflow session closed")?;
            waiting
                .await
                .map_err(|_| "workflow result was not persisted")?;
            if let Some(output) = registry
                .continue_action("workflows", result)
                .await
                .map_err(|error| error.to_string())?
            {
                pending.push(output);
            }
        }
    }
}
