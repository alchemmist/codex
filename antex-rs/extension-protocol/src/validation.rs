use std::collections::BTreeSet;

use crate::*;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid extension JSON-RPC envelope")]
    Encoding,
    #[error("extension {0} exceeds its limit")]
    Limit(&'static str),
    #[error("extension response id does not match the request")]
    MismatchedId,
    #[error("incompatible extension protocol version")]
    Version,
    #[error("invalid extension identifier")]
    Identifier,
    #[error("extension requested an ungranted capability")]
    Permission,
    #[error("extension error {code}: {message}")]
    Remote { code: i32, message: String },
}

fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

impl Manifest {
    pub fn validate(
        &self,
        expected_name: &str,
        granted: &BTreeSet<Capability>,
    ) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::Version);
        }
        if !identifier(&self.name) || self.name != expected_name {
            return Err(ProtocolError::Identifier);
        }
        if self.tools.len() > 32 || self.commands.len() > 64 || self.events.len() > 32 {
            return Err(ProtocolError::Limit("manifest"));
        }
        let process_caps: BTreeSet<_> = granted
            .iter()
            .copied()
            .filter(|cap| {
                matches!(
                    cap,
                    Capability::Network | Capability::WorkspaceRead | Capability::WorkspaceWrite
                )
            })
            .collect();
        let mut tools = BTreeSet::new();
        for tool in &self.tools {
            if !identifier(&tool.name) || !tools.insert(&tool.name) {
                return Err(ProtocolError::Identifier);
            }
            if tool.description.len() > MAX_TEXT_BYTES
                || !tool.parameters.is_object()
                || tool.parameters.to_string().len() > MAX_TEXT_BYTES
            {
                return Err(ProtocolError::Limit("tool definition"));
            }
            if !tool.permissions.is_subset(granted) || !process_caps.is_subset(&tool.permissions) {
                return Err(ProtocolError::Permission);
            }
        }
        let mut commands = BTreeSet::new();
        for command in &self.commands {
            if !identifier(&command.name) || !commands.insert(&command.name) {
                return Err(ProtocolError::Identifier);
            }
            if command.description.len() > 512 {
                return Err(ProtocolError::Limit("command description"));
            }
            if !command.permissions.is_subset(granted)
                || !process_caps.is_subset(&command.permissions)
            {
                return Err(ProtocolError::Permission);
            }
        }
        if self.events.iter().any(|event| !identifier(event)) {
            return Err(ProtocolError::Identifier);
        }
        Ok(())
    }
}

impl Output {
    pub fn validate(&self, granted: &BTreeSet<Capability>) -> Result<(), ProtocolError> {
        if self.text.len() > MAX_TEXT_BYTES
            || self.records.len() > 16
            || self.actions.len() > 16
            || self
                .records
                .iter()
                .any(|record| record.to_string().len() > MAX_TEXT_BYTES)
        {
            return Err(ProtocolError::Limit("result"));
        }
        if !self.records.is_empty() && !granted.contains(&Capability::Persist) {
            return Err(ProtocolError::Permission);
        }
        if (self.status.is_some() || self.panel.is_some()) && !granted.contains(&Capability::Ui) {
            return Err(ProtocolError::Permission);
        }
        if self
            .status
            .as_ref()
            .is_some_and(|status| status.len() > 256 || status.contains(['\n', '\r']))
            || self
                .panel
                .as_ref()
                .is_some_and(|panel| panel.title.len() > 128 || panel.text.len() > MAX_TEXT_BYTES)
        {
            return Err(ProtocolError::Limit("presentation"));
        }
        let mut ids = BTreeSet::new();
        for action in &self.actions {
            let (id, text, needed) = match action {
                Action::Shell {
                    id,
                    command,
                    workspace,
                    network,
                } => {
                    let mut needed = BTreeSet::from([Capability::Shell, Capability::WorkspaceRead]);
                    if *workspace == WorkspaceAccess::ReadWrite {
                        needed.insert(Capability::WorkspaceWrite);
                    }
                    if *network == NetworkAccess::Allowed {
                        needed.insert(Capability::Network);
                    }
                    (id, command.as_str(), needed)
                }
                Action::Agent { id, prompt, model } => {
                    if model.as_ref().is_some_and(|model| model.len() > 128) {
                        return Err(ProtocolError::Limit("model"));
                    }
                    (id, prompt.as_str(), BTreeSet::from([Capability::Agent]))
                }
                Action::Inspect { id, target } => (
                    id,
                    "",
                    BTreeSet::from([match target {
                        Inspection::Transcript => Capability::SessionRead,
                        Inspection::Context | Inspection::SystemPrompt => Capability::ContextRead,
                    }]),
                ),
                Action::TerminalLog { id, text } => {
                    (id, text.as_str(), BTreeSet::from([Capability::Ui]))
                }
            };
            if !identifier(id) || !ids.insert(id) {
                return Err(ProtocolError::Identifier);
            }
            if text.len() > MAX_TEXT_BYTES {
                return Err(ProtocolError::Limit("action"));
            }
            if !needed.is_subset(granted) {
                return Err(ProtocolError::Permission);
            }
        }
        Ok(())
    }
}
