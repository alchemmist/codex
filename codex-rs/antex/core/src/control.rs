use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use tokio_util::sync::CancellationToken;

use crate::UserInput;
use crate::validation;

const QUEUE_CAPACITY: usize = 32;
const QUEUED_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentCommand {
    Steer(UserInput),
    FollowUp(UserInput),
    Interrupt,
}

#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub struct CommandError {
    pub command: AgentCommand,
    pub reason: String,
}

#[derive(Clone)]
pub struct CommandSender {
    state: Arc<Mutex<State>>,
    pub(crate) cancel: CancellationToken,
}

struct State {
    open: bool,
    bytes: usize,
    steers: VecDeque<(UserInput, usize)>,
    followups: VecDeque<(UserInput, usize)>,
}

pub(crate) enum NextInput {
    Steer(UserInput),
    FollowUp(UserInput),
    Done,
}

impl CommandSender {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                open: true,
                bytes: 0,
                steers: VecDeque::new(),
                followups: VecDeque::new(),
            })),
            cancel: CancellationToken::new(),
        }
    }

    pub async fn send(&self, command: AgentCommand) -> Result<(), CommandError> {
        self.try_send(command)
    }

    pub fn try_send(&self, command: AgentCommand) -> Result<(), CommandError> {
        let input = match &command {
            AgentCommand::Interrupt => {
                self.cancel.cancel();
                return Ok(());
            }
            AgentCommand::Steer(input) | AgentCommand::FollowUp(input) => input,
        };
        let bytes = match validation::user(input) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(CommandError {
                    command,
                    reason: error.to_string(),
                });
            }
        };
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let reason = if !state.open || self.cancel.is_cancelled() {
            Some("run is closed")
        } else if state.bytes + bytes > QUEUED_BYTES
            || state.steers.len() + state.followups.len() >= QUEUE_CAPACITY
        {
            Some("command queue is full")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(CommandError {
                command,
                reason: reason.into(),
            });
        }
        state.bytes += bytes;
        match command {
            AgentCommand::Steer(input) => state.steers.push_back((input, bytes)),
            AgentCommand::FollowUp(input) => state.followups.push_back((input, bytes)),
            AgentCommand::Interrupt => unreachable!(),
        }
        Ok(())
    }

    pub(crate) fn take_steers(&self) -> Vec<UserInput> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut inputs = Vec::with_capacity(state.steers.len());
        while let Some((input, bytes)) = state.steers.pop_front() {
            state.bytes -= bytes;
            inputs.push(input);
        }
        inputs
    }

    pub(crate) fn next_or_close(&self) -> NextInput {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((input, bytes)) = state.steers.pop_front() {
            state.bytes -= bytes;
            NextInput::Steer(input)
        } else if let Some((input, bytes)) = state.followups.pop_front() {
            state.bytes -= bytes;
            NextInput::FollowUp(input)
        } else {
            state.open = false;
            NextInput::Done
        }
    }

    pub(crate) fn close(&self) -> Vec<AgentCommand> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.open = false;
        state.bytes = 0;
        let mut pending: Vec<_> = state
            .steers
            .drain(..)
            .map(|(input, _)| AgentCommand::Steer(input))
            .collect();
        pending.extend(
            state
                .followups
                .drain(..)
                .map(|(input, _)| AgentCommand::FollowUp(input)),
        );
        pending
    }
}
