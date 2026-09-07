use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use antex_core::ToolHost;
use antex_extension_protocol::CommandRun;
use antex_extension_protocol::Event;
use antex_extension_protocol::Output;
use tokio_util::sync::CancellationToken;

use crate::ExtensionConfig;
use crate::ExtensionError;
use crate::ExtensionLauncher;
use crate::ExtensionRequest;
use crate::ExtensionResponse;
use crate::ExtensionToolHost;
use crate::HostedExtension;
use crate::ManagedExtension;
use crate::ToolHostError;
use crate::discover;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryCommand {
    pub name: String,
    pub description: String,
}

struct CommandTarget {
    process: Arc<ManagedExtension>,
    remote_name: String,
}

struct EventTarget {
    name: String,
    process: Arc<ManagedExtension>,
}

pub struct ExtensionRegistry {
    tool_host: Arc<ExtensionToolHost>,
    commands: Vec<RegistryCommand>,
    command_targets: HashMap<String, CommandTarget>,
    event_targets: HashMap<String, Vec<EventTarget>>,
}

pub struct RegistryLoad {
    pub registry: ExtensionRegistry,
    pub failures: Vec<String>,
}

pub struct RegistryEventOutput {
    pub extension: String,
    pub output: Output,
}

pub struct RegistryEventDelivery {
    pub outputs: Vec<RegistryEventOutput>,
    pub failures: Vec<String>,
}

pub struct ExtensionRegistryConfig<'a> {
    pub home: &'a Path,
    pub workspace: &'a Path,
    pub project_trusted: bool,
    pub fallback: Arc<dyn ToolHost>,
    pub antex_version: &'a str,
    pub session_id: &'a str,
    pub launcher: Arc<dyn ExtensionLauncher>,
    pub states: &'a HashMap<String, serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(transparent)]
    Discovery(#[from] crate::DiscoveryError),
    #[error(transparent)]
    Tools(#[from] ToolHostError),
    #[error("duplicate extension command: {0}")]
    Command(String),
    #[error("unknown extension command: {0}")]
    UnknownCommand(String),
    #[error(transparent)]
    Extension(#[from] ExtensionError),
}

impl ExtensionRegistry {
    pub async fn load(config: ExtensionRegistryConfig<'_>) -> Result<RegistryLoad, RegistryError> {
        let definitions = discover(config.home, config.workspace, config.project_trusted)?;
        let mut extensions = Vec::new();
        let mut failures = Vec::new();
        for definition in definitions {
            let mut extension_config = ExtensionConfig::new(
                definition.name.clone(),
                definition.program,
                config.workspace.to_path_buf(),
            );
            extension_config.arguments = definition.arguments.into_iter().map(Into::into).collect();
            extension_config.antex_version = config.antex_version.into();
            extension_config.session_id = config.session_id.into();
            extension_config.capabilities = definition.capabilities;
            extension_config.state = config
                .states
                .get(&definition.name)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            extension_config.launcher = Some(Arc::clone(&config.launcher));
            match ManagedExtension::launch(extension_config).await {
                Ok(process) => {
                    let process = Arc::new(process);
                    let manifest = process.manifest().await?;
                    extensions.push(HostedExtension { process, manifest });
                }
                Err(error) => failures.push(format!("{}: {error}", definition.name)),
            }
        }
        let mut commands = Vec::new();
        let mut command_targets = HashMap::new();
        let mut event_targets: HashMap<String, Vec<EventTarget>> = HashMap::new();
        for extension in &extensions {
            for command in &extension.manifest.commands {
                let name = command.name.clone();
                if command_targets
                    .insert(
                        name.clone(),
                        CommandTarget {
                            process: Arc::clone(&extension.process),
                            remote_name: command.name.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(RegistryError::Command(name));
                }
                commands.push(RegistryCommand {
                    name,
                    description: command.description.clone(),
                });
            }
            for event in &extension.manifest.events {
                event_targets
                    .entry(event.clone())
                    .or_default()
                    .push(EventTarget {
                        name: extension.manifest.name.clone(),
                        process: Arc::clone(&extension.process),
                    });
            }
        }
        commands.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(RegistryLoad {
            registry: Self {
                tool_host: Arc::new(ExtensionToolHost::new(config.fallback, extensions)?),
                commands,
                command_targets,
                event_targets,
            },
            failures,
        })
    }

    pub fn tool_host(&self) -> Arc<dyn ToolHost> {
        self.tool_host.clone()
    }

    pub fn commands(&self) -> &[RegistryCommand] {
        &self.commands
    }

    pub async fn run_command(
        &self,
        name: &str,
        arguments: String,
        cancellation: CancellationToken,
    ) -> Result<Output, RegistryError> {
        let target = self
            .command_targets
            .get(name)
            .ok_or_else(|| RegistryError::UnknownCommand(name.into()))?;
        match target
            .process
            .request(
                ExtensionRequest::Command(CommandRun {
                    name: target.remote_name.clone(),
                    arguments,
                }),
                cancellation,
            )
            .await?
        {
            ExtensionResponse::Output(output) => Ok(output),
            ExtensionResponse::Notified => Err(ExtensionError::Protocol(
                antex_extension_protocol::ProtocolError::Encoding,
            )
            .into()),
        }
    }

    pub async fn notify(&self, event: Event) -> RegistryEventDelivery {
        let Some(targets) = self.event_targets.get(&event.name) else {
            return RegistryEventDelivery {
                outputs: Vec::new(),
                failures: Vec::new(),
            };
        };
        let mut outputs = Vec::new();
        let mut failures = Vec::new();
        for target in targets {
            match target
                .process
                .request(
                    ExtensionRequest::Event(event.clone()),
                    CancellationToken::new(),
                )
                .await
            {
                Ok(ExtensionResponse::Output(output)) => outputs.push(RegistryEventOutput {
                    extension: target.name.clone(),
                    output,
                }),
                Ok(ExtensionResponse::Notified) => {}
                Err(error) => failures.push(format!("{}: {error}", target.name)),
            }
        }
        RegistryEventDelivery { outputs, failures }
    }
}
