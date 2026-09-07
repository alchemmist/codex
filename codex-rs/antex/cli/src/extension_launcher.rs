use std::sync::Arc;

use antex_extension_host::ExtensionLaunch;
use antex_extension_host::ExtensionLauncher;
use antex_extension_protocol::Capability;
use antex_runtime::ExtensionSandbox;
use antex_runtime::ExtensionWorkspace;
use futures::future::BoxFuture;

pub(crate) struct RuntimeExtensionLauncher {
    sandbox: Arc<ExtensionSandbox>,
}

impl RuntimeExtensionLauncher {
    pub(crate) fn new(sandbox: ExtensionSandbox) -> Self {
        Self {
            sandbox: Arc::new(sandbox),
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
            self.sandbox
                .command(
                    launch.program,
                    launch.arguments,
                    workspace,
                    launch.capabilities.contains(&Capability::Network),
                )
                .await
        })
    }
}
