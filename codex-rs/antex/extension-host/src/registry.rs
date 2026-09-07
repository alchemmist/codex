use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use antex_core::ToolHost;
use antex_extension_protocol::CommandRun;
use antex_extension_protocol::Output;
use tokio_util::sync::CancellationToken;

use crate::ExtensionConfig;
use crate::ExtensionError;
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

pub struct ExtensionRegistry {
    tool_host: Arc<ExtensionToolHost>,
    commands: Vec<RegistryCommand>,
    command_targets: HashMap<String, CommandTarget>,
}

pub struct RegistryLoad {
    pub registry: ExtensionRegistry,
    pub failures: Vec<String>,
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
    pub async fn load(
        home: &Path,
        workspace: &Path,
        project_trusted: bool,
        fallback: Arc<dyn ToolHost>,
        antex_version: &str,
        session_id: &str,
    ) -> Result<RegistryLoad, RegistryError> {
        let definitions = discover(home, workspace, project_trusted)?;
        let mut extensions = Vec::new();
        let mut failures = Vec::new();
        for definition in definitions {
            let mut config = ExtensionConfig::new(
                definition.name.clone(),
                definition.program,
                workspace.to_path_buf(),
            );
            config.arguments = definition.arguments.into_iter().map(Into::into).collect();
            config.antex_version = antex_version.into();
            config.session_id = session_id.into();
            config.capabilities = definition.capabilities;
            match ManagedExtension::launch(config).await {
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
        for extension in &extensions {
            for command in &extension.manifest.commands {
                let name = format!("{}__{}", extension.manifest.name, command.name);
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
        }
        commands.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(RegistryLoad {
            registry: Self {
                tool_host: Arc::new(ExtensionToolHost::new(fallback, extensions)?),
                commands,
                command_targets,
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
}
