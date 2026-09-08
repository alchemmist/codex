use std::sync::Arc;

use antex_extension_host::ExtensionLaunch;
use antex_extension_host::ExtensionLauncher;
use antex_extension_protocol::Capability;
use antex_runtime::ExtensionSandbox;
use antex_runtime::ExtensionWorkspace;
use futures::future::BoxFuture;

pub(crate) struct RuntimeExtensionLauncher {
    sandbox: Arc<ExtensionSandbox>,
    workflows: std::path::PathBuf,
    project_trusted: bool,
}

impl RuntimeExtensionLauncher {
    pub(crate) fn new(
        sandbox: ExtensionSandbox,
        workflows: std::path::PathBuf,
        project_trusted: bool,
    ) -> Self {
        Self {
            sandbox: Arc::new(sandbox),
            workflows,
            project_trusted,
        }
    }
}

impl ExtensionLauncher for RuntimeExtensionLauncher {
    fn command<'a>(
        &'a self,
        launch: ExtensionLaunch<'a>,
    ) -> BoxFuture<'a, std::io::Result<tokio::process::Command>> {
        Box::pin(async move {
            let workspace = if launch.capabilities.contains(&Capability::WorkspaceWrite) {
                ExtensionWorkspace::ReadWrite
            } else if launch.capabilities.contains(&Capability::WorkspaceRead) {
                ExtensionWorkspace::ReadOnly
            } else {
                ExtensionWorkspace::Hidden
            };
            let mut command = self
                .sandbox
                .command(
                    launch.program,
                    launch.arguments,
                    workspace,
                    launch.capabilities.contains(&Capability::Network),
                )
                .await?;
            command.env("ANTEX_WORKFLOWS_DIR", &self.workflows);
            command.env(
                "ANTEX_TRUST_PROJECT_WORKFLOWS",
                if self.project_trusted { "1" } else { "0" },
            );
            Ok(command)
        })
    }
}
