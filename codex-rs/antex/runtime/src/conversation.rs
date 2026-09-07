use std::io;

use antex_core::AgentEvent;
use antex_core::Message;
use uuid::Uuid;

use crate::Session;
use crate::SessionMessage;

pub struct Conversation {
    session: Session,
    entries: Vec<SessionMessage>,
}

impl Conversation {
    pub fn new(mut session: Session) -> io::Result<Self> {
        session.recover_pending_tools()?;
        let entries = session.active_path()?;
        Ok(Self { session, entries })
    }

    pub fn id(&self) -> Uuid {
        self.session.id()
    }

    pub fn entries(&self) -> &[SessionMessage] {
        &self.entries
    }

    pub fn messages(&self) -> Vec<Message> {
        self.entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect()
    }

    pub fn branch(&mut self, parent: Uuid) -> io::Result<()> {
        self.session.branch(parent)?;
        self.entries = self.session.active_path()?;
        Ok(())
    }

    pub fn record(&mut self, event: &AgentEvent) -> io::Result<()> {
        match event {
            AgentEvent::MessageCommitted(message) => {
                let record_id = self.session.append(message)?;
                self.entries.push(SessionMessage {
                    record_id,
                    message: message.clone(),
                });
            }
            AgentEvent::ContextCheckpoint(checkpoint) => {
                let retained = checkpoint
                    .retained
                    .iter()
                    .map(|index| {
                        self.entries
                            .get(*index)
                            .map(|entry| entry.record_id)
                            .ok_or_else(|| io::Error::other("missing retained session record"))
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                let tail = self
                    .entries
                    .get(checkpoint.tail_start)
                    .ok_or_else(|| io::Error::other("missing session checkpoint tail"))?
                    .record_id;
                self.session
                    .checkpoint(checkpoint.summary.clone(), &retained, tail)?;
                self.entries = self.session.active_path()?;
            }
            AgentEvent::Finished { .. } => self.session.finish_turn()?,
            AgentEvent::Interaction { .. }
            | AgentEvent::ToolProgress { .. }
            | AgentEvent::Quota(_)
            | AgentEvent::TextDelta(_)
            | AgentEvent::ReasoningDelta(_)
            | AgentEvent::ToolStarted(_)
            | AgentEvent::Usage(_)
            | AgentEvent::TurnCompleted
            | AgentEvent::Error(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;
