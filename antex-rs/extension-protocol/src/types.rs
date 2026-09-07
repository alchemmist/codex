use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    Network,
    WorkspaceRead,
    WorkspaceWrite,
    Shell,
    Agent,
    SessionRead,
    ContextRead,
    Persist,
    Ui,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Initialize {
    pub protocol_version: u32,
    pub antex_version: String,
    pub session_id: String,
    pub cwd: PathBuf,
    pub capabilities: BTreeSet<Capability>,
    pub state: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub protocol_version: u32,
    pub name: String,
    pub tools: Vec<Tool>,
    pub commands: Vec<Command>,
    pub events: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub permissions: BTreeSet<Capability>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub permissions: BTreeSet<Capability>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRun {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Event {
    pub name: String,
    pub data: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Output {
    pub text: String,
    #[serde(default)]
    pub records: Vec<Value>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub panel: Option<Panel>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Panel {
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    Shell {
        id: String,
        command: String,
        workspace: WorkspaceAccess,
        network: NetworkAccess,
    },
    Agent {
        id: String,
        prompt: String,
        model: Option<String>,
    },
    Inspect {
        id: String,
        target: Inspection,
    },
    TerminalLog {
        id: String,
        text: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkAccess {
    Denied,
    Allowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Inspection {
    Transcript,
    Context,
    SystemPrompt,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionResult {
    pub id: String,
    pub succeeded: bool,
    pub data: Value,
}
