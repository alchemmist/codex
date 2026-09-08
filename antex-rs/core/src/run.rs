use std::sync::Arc;

use tokio::sync::mpsc;

use crate::AgentCommand;
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
    let mut pending = Vec::new();
    let result = drive(
        provider.as_ref(),
        tools.as_ref(),
        hook.as_deref(),
        input,
        &events,
        &commands,
        &mut pending,
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
    pending.extend(commands.close());
    let _ = events.send(AgentEvent::Finished { reason, pending }).await;
}

async fn drive<P: ModelProvider>(
    provider: &P,
    tools: &dyn ToolHost,
    hook: Option<&dyn ContextHook>,
    input: TurnInput,
    events: &mpsc::Sender<AgentEvent>,
    commands: &CommandSender,
    pending: &mut Vec<AgentCommand>,
) -> Result<(), ProviderError> {
    let cancel = &commands.cancel;
    let mut history = input.history;
    let mut scope = input.input.tool_scope.clone();
    commit_input(
        AgentCommand::FollowUp(input.input),
        InputCommit::Initial,
        &mut history,
        events,
        cancel,
        pending,
    )
    .await?;
    if input.model.is_empty()
        || input.model.len() > 128
        || input
            .reasoning
            .as_ref()
            .is_some_and(|value| value.len() > 64)
    {
        return Err(error(ErrorKind::Limit, "invalid model selection"));
    }
    for _ in 0..64 {
        for _ in 0..crate::control::QUEUE_CAPACITY {
            let Some(input) = commands.take_steer() else {
                break;
            };
            commit_input(
                AgentCommand::Steer(input),
                InputCommit::Queued,
                &mut history,
                events,
                cancel,
                pending,
            )
            .await?;
        }
        let toolset = ToolSet::new(tools.definitions(&scope))?;
        let prepared = if let Some(hook) = hook {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(error(ErrorKind::Cancelled, "run interrupted")),
                _ = events.closed() => return Err(error(ErrorKind::Cancelled, "event receiver closed")),
                result = hook.prepare(&history) => result?,
            }
        } else {
            history.clone().into()
        };
        validation::messages(&prepared.messages)?;
        if let Some(checkpoint) = prepared.checkpoint {
            if checkpoint.summary.kind() != crate::ContextKind::Summary
                || checkpoint.retained.len() > 8
                || checkpoint.tail_start >= history.len()
                || checkpoint
                    .retained
                    .iter()
                    .any(|index| *index >= checkpoint.tail_start)
                || checkpoint
                    .retained
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
            {
                return Err(error(ErrorKind::Protocol, "invalid context checkpoint"));
            }
            let projected = std::iter::once(Message::Context(checkpoint.summary.clone()))
                .chain(
                    checkpoint
                        .retained
                        .iter()
                        .map(|index| history[*index].clone()),
                )
                .chain(history[checkpoint.tail_start..].iter().cloned())
                .collect::<Vec<_>>();
            if !prepared.messages.ends_with(&projected) {
                return Err(error(
                    ErrorKind::Protocol,
                    "context checkpoint does not match the prepared history",
                ));
            }
            if checkpoint
                .retained
                .iter()
                .any(|index| !matches!(history[*index], Message::User(_)))
                || matches!(history[checkpoint.tail_start], Message::Tool(_))
            {
                return Err(error(
                    ErrorKind::Protocol,
                    "checkpoint splits a tool exchange or retains an invalid request",
                ));
            }
            if history
                .iter()
                .rposition(|message| matches!(message, Message::User(_)))
                .is_some_and(|index| {
                    index < checkpoint.tail_start && !checkpoint.retained.contains(&index)
                })
            {
                return Err(error(
                    ErrorKind::Protocol,
                    "checkpoint discarded the latest user request",
                ));
            }
            if checkpoint.usage != Usage::default() {
                emit(events, cancel, AgentEvent::Usage(checkpoint.usage)).await?;
            }
            emit(
                events,
                cancel,
                AgentEvent::ContextCheckpoint(checkpoint.clone()),
            )
            .await?;
            history = projected;
        } else if !prepared.messages.ends_with(&history) {
            return Err(error(
                ErrorKind::Protocol,
                "context hook changed history without a checkpoint",
            ));
        }
        let messages = prepared.messages;
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
        validation::history(&history)?;
        events
            .send(AgentEvent::MessageCommitted(message))
            .await
            .map_err(|_| error(ErrorKind::Cancelled, "event receiver closed"))?;
        let _ = emit(events, cancel, AgentEvent::Usage(response.usage)).await;
        if response.calls.is_empty() {
            emit(events, cancel, AgentEvent::TurnCompleted).await?;
            match commands.next_or_close() {
                NextInput::Steer(input) => {
                    commit_input(
                        AgentCommand::Steer(input),
                        InputCommit::Queued,
                        &mut history,
                        events,
                        cancel,
                        pending,
                    )
                    .await?;
                }
                NextInput::FollowUp(input) => {
                    scope = input.tool_scope.clone();
                    commit_input(
                        AgentCommand::FollowUp(input),
                        InputCommit::Queued,
                        &mut history,
                        events,
                        cancel,
                        pending,
                    )
                    .await?;
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
                            events: events.clone(),
                            call_id: call.id.clone(),
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
        validation::history(&history)?;
        if cancel.is_cancelled() {
            return Err(error(ErrorKind::Cancelled, "run interrupted"));
        }
    }
    Err(error(
        ErrorKind::Limit,
        "run exceeded its model request budget",
    ))
}

enum InputCommit {
    Initial,
    Queued,
}

async fn commit_input(
    command: AgentCommand,
    mode: InputCommit,
    history: &mut Vec<Message>,
    events: &mpsc::Sender<AgentEvent>,
    cancel: &tokio_util::sync::CancellationToken,
    pending: &mut Vec<AgentCommand>,
) -> Result<(), ProviderError> {
    pending.push(command.clone());
    if matches!(mode, InputCommit::Queued) && cancel.is_cancelled() {
        return Err(error(ErrorKind::Cancelled, "input cancelled before commit"));
    }
    let input = match command {
        AgentCommand::Steer(input) | AgentCommand::FollowUp(input) => input,
        AgentCommand::Interrupt => {
            return Err(error(ErrorKind::Protocol, "interrupt is not user input"));
        }
    };
    let message = Message::User(input);
    history.push(message.clone());
    validation::history(history)?;
    events
        .send(AgentEvent::MessageCommitted(message))
        .await
        .map_err(|_| error(ErrorKind::Cancelled, "event receiver closed"))?;
    pending.clear();
    Ok(())
}
