#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::sync::Arc;

use antex_extension_host::Extension;
use antex_extension_host::ExtensionConfig;
use antex_extension_host::ExtensionLaunch;
use antex_extension_host::ExtensionLauncher;
use antex_extension_host::ExtensionRequest;
use antex_extension_host::ExtensionResponse;
use antex_extension_protocol::Capability;
use antex_extension_protocol::ToolCall;
use antex_runtime::ExtensionSandbox;
use antex_runtime::ExtensionWorkspace;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct SandboxedLauncher(ExtensionSandbox);

impl ExtensionLauncher for SandboxedLauncher {
    fn command<'a>(
        &'a self,
        launch: ExtensionLaunch<'a>,
    ) -> BoxFuture<'a, std::io::Result<tokio::process::Command>> {
        Box::pin(async move {
            self.0
                .command(
                    launch.program,
                    launch.arguments,
                    ExtensionWorkspace::Hidden,
                    false,
                )
                .await
        })
    }
}

#[tokio::test]
async fn stdio_mcp_tools_run_end_to_end_inside_the_extension_sandbox() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../extensions/mcp");
    let bridge = root.join("antex_ext_mcp.py").canonicalize().unwrap();
    let server = root.join("tests/fake_server.py").canonicalize().unwrap();
    let bubblewrap = std::env::var_os("ANTEX_BWRAP")
        .map(Into::into)
        .unwrap_or_else(|| "/usr/bin/bwrap".into());
    let sandbox = ExtensionSandbox::new(home.path(), workspace.path(), bubblewrap, &[]).unwrap();
    let mut config = ExtensionConfig::new("fixture", bridge, workspace.path().into());
    config.arguments = vec![
        "--name".into(),
        "fixture".into(),
        "--".into(),
        "/usr/bin/python3".into(),
        server.into_os_string(),
    ];
    config.capabilities.insert(Capability::Shell);
    config.launcher = Some(Arc::new(SandboxedLauncher(sandbox)));
    let extension = Extension::launch(config).await.unwrap();
    let tool = extension.manifest().tools.first().unwrap();
    assert!(tool.name.starts_with("echo_"));
    let response = extension
        .request(
            ExtensionRequest::Tool(ToolCall {
                name: tool.name.clone(),
                arguments: json!({"text":"sandboxed-mcp"}),
            }),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let ExtensionResponse::Output(output) = response else {
        panic!("MCP tool returned a notification");
    };
    assert_eq!(output.text, "sandboxed-mcp");
    extension.shutdown().await.unwrap();
}
