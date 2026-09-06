use std::sync::Arc;

use tokio::sync::mpsc;

use crate::AgentEvent;
use crate::CommandSender;
use crate::ContextHook;
use crate::ErrorKind;
use crate::FinishReason;
use crate::Message;
use crate::ModelProvider;
use crate::ModelRequest;
use crate::ProviderError;
use crate::ToolContext;
use crate::ToolHost;
use crate::ToolOutcome;
use crate::ToolOutput;
use crate::TurnInput;
use crate::Usage;
use crate::control::NextInput;
use crate::response;
use crate::response::Response;
use crate::response::emit;
use crate::response::error;
use crate::toolset::ToolSet;
use crate::validation;

pub(crate) async fn run<P: ModelProvider>(
    provider: Arc<P>,
    tools: Arc<dyn ToolHost>,
    hook: Option<Arc<dyn ContextHook>>,
    input: TurnInput,
    events: mpsc::Sender<AgentEvent>,
    commands: CommandSender,
) {
    let result = drive(
        provider.as_ref(),
        tools.as_ref(),
        hook.as_deref(),
        input,
        &events,
        &commands,
    )
    .await;
    let reason = match result {
        Ok(()) => FinishReason::Completed,
        Err(error) if error.kind == ErrorKind::Cancelled => FinishReason::Interrupted,
        Err(error) => {
            let _ = events.send(AgentEvent::Error(error)).await;
            FinishReason::Failed
        }
    };
    let pending = commands.close();
    let _ = events.send(AgentEvent::Finished { reason, pending }).await;
}

async fn drive<P: ModelProvider>(
    provider: &P,
    tools: &dyn ToolHost,
    hook: Option<&dyn ContextHook>,
    input: TurnInput,
    events: &mpsc::Sender<AgentEvent>,
    commands: &CommandSender,
) -> Result<(), ProviderError> {
    let cancel = &commands.cancel;
    if input.model.is_empty()
        || input.model.len() > 128
        || input
            .reasoning
            .as_ref()
            .is_some_and(|value| value.len() > 64)
    {
        return Err(error(ErrorKind::Limit, "invalid model selection"));
    }
    let mut history = input.history;
    let mut scope = input.input.tool_scope.clone();
    history.push(Message::User(input.input));
    validation::messages(&history)?;
    emit(
        events,
        cancel,
        AgentEvent::MessageCommitted(history.last().unwrap().clone()),
    )
    .await?;
    for _ in 0..64 {
        for input in commands.take_steers() {
            let message = Message::User(input);
            history.push(message.clone());
            validation::messages(&history)?;
            emit(events, cancel, AgentEvent::MessageCommitted(message)).await?;
        }
        let toolset = ToolSet::new(tools.definitions(&scope))?;
        let messages = if let Some(hook) = hook {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(error(ErrorKind::Cancelled, "run interrupted")),
                _ = events.closed() => return Err(error(ErrorKind::Cancelled, "event receiver closed")),
                result = hook.prepare(&history) => result?,
            }
        } else {
            history.clone()
        };
        validation::messages(&messages)?;
        let request = ModelRequest {
            model: input.model.clone(),
            reasoning: input.reasoning.clone(),
            messages,
            tools: toolset.definitions.clone(),
        };
        let mut response = Response {
            content: Vec::new(),
            calls: Vec::new(),
            usage: Usage::default(),
        };
        let result = response::collect(provider, request, events, cancel, &mut response).await;
        if let Err(error) = result {
            let partial = Message::Assistant {
                content: response.content,
                tool_calls: Vec::new(),
            };
            if validation::messages(std::slice::from_ref(&partial)).is_ok()
                && let Message::Assistant { content, .. } = &partial
                && !content.is_empty()
            {
                let _ = events.send(AgentEvent::MessageCommitted(partial)).await;
            }
            return Err(error);
        }
        let message = Message::Assistant {
            content: response.content,
            tool_calls: response.calls.clone(),
        };
        history.push(message.clone());
        validation::messages(&history)?;
        emit(events, cancel, AgentEvent::MessageCommitted(message)).await?;
        let _ = emit(events, cancel, AgentEvent::Usage(response.usage)).await;
        if response.calls.is_empty() {
            emit(events, cancel, AgentEvent::TurnCompleted).await?;
            match commands.next_or_close() {
                NextInput::Steer(input) => {
                    let message = Message::User(input);
                    history.push(message.clone());
                    emit(events, cancel, AgentEvent::MessageCommitted(message)).await?;
                }
                NextInput::FollowUp(input) => {
                    scope = input.tool_scope.clone();
                    let message = Message::User(input);
                    history.push(message.clone());
                    emit(events, cancel, AgentEvent::MessageCommitted(message)).await?;
                }
                NextInput::Done => return Ok(()),
            }
            continue;
        }
        for call in response.calls {
            let output = if cancel.is_cancelled() {
                ToolOutput::new(
                    call.id.clone(),
                    ToolOutcome::Cancelled,
                    "tool call cancelled".into(),
                )
            } else {
                match toolset.validate(&call) {
                    Err(message) => {
                        ToolOutput::new(call.id.clone(), ToolOutcome::Failure, message.into())
                    }
                    Ok(validated) => {
                        let _ =
                            emit(events, cancel, AgentEvent::ToolStarted(validated.clone())).await;
                        let context = ToolContext {
                            cancellation: cancel.clone(),
                        };
                        let mut output = tokio::select! {
                            biased;
                            _ = cancel.cancelled() => ToolOutput::new(call.id.clone(), ToolOutcome::Cancelled, "tool call cancelled".into()),
                            _ = events.closed() => return Err(error(ErrorKind::Cancelled, "event receiver closed")),
                            output = tools.execute(validated, context) => output,
                        };
                        output.call_id = call.id.clone();
                        output
                    }
                }
            };
            let message = Message::Tool(output);
            history.push(message.clone());
            events
                .send(AgentEvent::MessageCommitted(message))
                .await
                .map_err(|_| error(ErrorKind::Cancelled, "event receiver closed"))?;
        }
        validation::messages(&history)?;
        if cancel.is_cancelled() {
            return Err(error(ErrorKind::Cancelled, "run interrupted"));
        }
    }
    Err(error(
        ErrorKind::Limit,
        "run exceeded its model request budget",
    ))
}
