use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::AgentCommand;
use crate::AgentEvent;
use crate::AgentRun;
use crate::CommandSender;
use crate::ContextHook;
use crate::FinishReason;
use crate::ModelProvider;
use crate::ToolHost;
use crate::TurnInput;

pub struct Agent<P> {
    provider: Arc<P>,
    tools: Arc<dyn ToolHost>,
    hook: Option<Arc<dyn ContextHook>>,
    active: Option<(CancellationToken, JoinHandle<()>)>,
}

impl<P: ModelProvider + 'static> Agent<P> {
    pub fn new(provider: P, tools: Arc<dyn ToolHost>) -> Self {
        Self {
            provider: Arc::new(provider),
            tools,
            hook: None,
            active: None,
        }
    }

    pub fn with_context_hook(mut self, hook: Arc<dyn ContextHook>) -> Self {
        self.hook = Some(hook);
        self
    }

    pub fn start(&mut self, input: TurnInput) -> AgentRun {
        let commands = CommandSender::new();
        let (sender, events) = mpsc::channel(64);
        if self
            .active
            .as_ref()
            .is_some_and(|(_, task)| !task.is_finished())
        {
            commands.close();
            let _ = sender.try_send(AgentEvent::Error(crate::response::error(
                crate::ErrorKind::Protocol,
                "agent already has an active run",
            )));
            let _ = sender.try_send(AgentEvent::Finished {
                reason: FinishReason::Failed,
                pending: vec![AgentCommand::FollowUp(input.input)],
            });
            return AgentRun { events, commands };
        }
        let provider = self.provider.clone();
        let tools = self.tools.clone();
        let hook = self.hook.clone();
        let control = commands.clone();
        let task = tokio::spawn(async move {
            crate::run::run(provider, tools, hook, input, sender, control).await;
        });
        self.active = Some((commands.cancel.clone(), task));
        AgentRun { events, commands }
    }
}

impl<P> Drop for Agent<P> {
    fn drop(&mut self) {
        if let Some((cancel, _)) = &self.active {
            cancel.cancel();
        }
    }
}
