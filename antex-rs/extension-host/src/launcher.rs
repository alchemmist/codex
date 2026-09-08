use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

use antex_extension_protocol::Capability;
use futures::future::BoxFuture;
use tokio::process::Command;

pub struct ExtensionLaunch<'a> {
    pub program: &'a Path,
    pub arguments: &'a [OsString],
    pub cwd: &'a Path,
    pub capabilities: &'a BTreeSet<Capability>,
}

/// Creates a capability-constrained child command while leaving stdin available for the protocol.
pub trait ExtensionLauncher: Send + Sync {
    fn command<'a>(
        &'a self,
        launch: ExtensionLaunch<'a>,
    ) -> BoxFuture<'a, std::io::Result<Command>>;
}
