use antex_core::AgentCommand;
use antex_core::Message;
use serde_json::Value;
use serde_json::json;

use super::*;

impl Session {
    pub fn pending_commands(&mut self) -> io::Result<Vec<AgentCommand>> {
        let mut next = Some(self.head);
        let mut consumed = Vec::new();
        let mut pending = Vec::new();
        while let Some(id) = next {
            let index = &self.records[&id];
            next = index.parent;
            if index.kind == Kind::User {
                consumed.push(id);
            }
            if index.kind == Kind::Pending {
                let (record, _) = self.read_record(id)?;
                let items = record
                    .payload
                    .as_array()
                    .filter(|items| items.len() <= 32)
                    .ok_or_else(|| io::Error::other("invalid pending command record"))?;
                for item in items {
                    let Message::User(input) = session_codec::decode(&item["input"])? else {
                        return Err(io::Error::other("invalid pending input"));
                    };
                    pending.push(match item["kind"].as_str() {
                        Some("steer") => AgentCommand::Steer(input),
                        Some("follow_up") => AgentCommand::FollowUp(input),
                        _ => return Err(io::Error::other("invalid pending command kind")),
                    });
                }
                break;
            }
        }
        for id in consumed.into_iter().rev() {
            if pending.is_empty() {
                break;
            }
            let (record, _) = self.read_record(id)?;
            let message = session_codec::decode(&record.payload)?;
            if let Message::User(input) = message
                && let Some(index) = pending.iter().position(|command| match command {
                    AgentCommand::Steer(pending) | AgentCommand::FollowUp(pending) => {
                        *pending == input
                    }
                    AgentCommand::Interrupt => false,
                })
            {
                pending.remove(index);
            }
        }
        Ok(pending)
    }

    pub fn save_pending_commands(&mut self, commands: &[AgentCommand]) -> io::Result<()> {
        if commands.len() > 32 {
            return Err(io::Error::other("too many pending commands"));
        }
        let mut bytes = 0;
        let payload = commands.iter().map(|command| {
            let (kind, input) = match command {
                AgentCommand::Steer(input) => ("steer", input),
                AgentCommand::FollowUp(input) => ("follow_up", input),
                AgentCommand::Interrupt => return Err(io::Error::other("interrupt is not a pending input")),
            };
            bytes += antex_core::context_size(&[Message::User(input.clone())]).map_err(io::Error::other)?;
            Ok(json!({"kind":kind,"input":session_codec::encode(&Message::User(input.clone()))}))
        }).collect::<io::Result<Vec<Value>>>()?;
        if bytes > 4 * 1024 * 1024 {
            return Err(io::Error::other(
                "pending commands exceed their byte budget",
            ));
        }
        self.append_record(Kind::Pending, Value::Array(payload), Some(self.head))?;
        self.finish_turn()
    }
}
