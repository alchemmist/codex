use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use antex_extension_host::Extension;
use antex_extension_host::ExtensionConfig;
use antex_extension_host::ExtensionError;
use antex_extension_host::ExtensionLaunch;
use antex_extension_host::ExtensionLauncher;
use antex_extension_host::ExtensionRequest;
use antex_extension_host::ExtensionResponse;
use antex_extension_host::ManagedExtension;
use antex_extension_protocol::Capability;
use antex_extension_protocol::CommandRun;
use antex_extension_protocol::ToolCall;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct DirectLauncher;

impl ExtensionLauncher for DirectLauncher {
    fn command<'a>(
        &'a self,
        launch: ExtensionLaunch<'a>,
    ) -> BoxFuture<'a, std::io::Result<tokio::process::Command>> {
        Box::pin(async move {
            let mut command = tokio::process::Command::new(launch.program);
            command.args(launch.arguments).current_dir(launch.cwd);
            Ok(command)
        })
    }
}

#[tokio::test]
async fn launching_without_a_sandbox_launcher_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let result = Extension::launch(ExtensionConfig::new(
        "fixture",
        "/does/not/matter",
        directory.path().into(),
    ))
    .await;
    assert!(matches!(result, Err(ExtensionError::MissingLauncher)));
}

fn fixture_config() -> ExtensionConfig {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance.py");
    let mut config =
        ExtensionConfig::new("fixture", "/usr/bin/env", fixture.parent().unwrap().into());
    config.arguments = vec!["python3".into(), fixture.into_os_string()];
    config.session_id = "session-1".into();
    config.timeout = Duration::from_secs(2);
    config.launcher = Some(std::sync::Arc::new(DirectLauncher));
    config
}

async fn assert_conformance(config: ExtensionConfig) {
    let extension = Extension::launch(config).await.unwrap();
    assert_eq!(extension.manifest().tools[0].name, "echo");
    assert_eq!(extension.manifest().commands[0].name, "hello");

    let response = extension
        .request(
            ExtensionRequest::Tool(ToolCall {
                name: "echo".into(),
                arguments: json!({"text":"bounded"}),
            }),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let ExtensionResponse::Output(output) = response else {
        panic!("tool returned a notification response");
    };
    assert_eq!(output.text, "bounded");

    let response = extension
        .request(
            ExtensionRequest::Command(CommandRun {
                name: "hello".into(),
                arguments: String::new(),
            }),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let ExtensionResponse::Output(output) = response else {
        panic!("command returned a notification response");
    };
    assert_eq!(output.text, "hello");
    extension.shutdown().await.unwrap();
}

#[tokio::test]
async fn python_fixture_registers_and_serves_tools_and_commands() {
    assert_conformance(fixture_config()).await;
}

#[tokio::test]
async fn rust_fixture_passes_the_same_conformance_contract() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("fixture");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance.rs");
    let status = Command::new("rustc")
        .args(["--edition=2024", "-o"])
        .arg(&program)
        .arg(source)
        .status()
        .unwrap();
    assert!(status.success());
    assert_conformance(
        ExtensionConfig::new("fixture", program, directory.path().into())
            .with_launcher(std::sync::Arc::new(DirectLauncher)),
    )
    .await;
}

#[tokio::test]
async fn timeout_terminates_only_the_failed_extension() {
    let mut config = fixture_config();
    config.timeout = Duration::from_millis(200);
    let extension = Extension::launch(config).await.unwrap();
    let result = extension
        .request(
            ExtensionRequest::Tool(ToolCall {
                name: "echo".into(),
                arguments: json!({"text":"stall"}),
            }),
            CancellationToken::new(),
        )
        .await;
    assert!(matches!(result, Err(ExtensionError::Timeout)));
    let healthy = Extension::launch(fixture_config()).await.unwrap();
    assert_eq!(healthy.manifest().name, "fixture");
    healthy.shutdown().await.unwrap();
}

#[tokio::test]
async fn model_tool_cannot_request_an_agent_action() {
    let mut config = fixture_config();
    config.capabilities.insert(Capability::Agent);
    let extension = Extension::launch(config).await.unwrap();
    let result = extension
        .request(
            ExtensionRequest::Tool(ToolCall {
                name: "echo".into(),
                arguments: json!({"text":"agent"}),
            }),
            CancellationToken::new(),
        )
        .await;
    assert!(matches!(result, Err(ExtensionError::AgentOrigin)));
    extension.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_request_is_not_replayed_and_next_request_restarts_the_process() {
    let managed = ManagedExtension::launch(fixture_config()).await.unwrap();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let result = managed
        .request(
            ExtensionRequest::Tool(ToolCall {
                name: "echo".into(),
                arguments: json!({"text":"never-replay"}),
            }),
            cancelled,
        )
        .await;
    assert!(matches!(result, Err(ExtensionError::Cancelled)));

    let response = managed
        .request(
            ExtensionRequest::Tool(ToolCall {
                name: "echo".into(),
                arguments: json!({"text":"after-restart"}),
            }),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let ExtensionResponse::Output(output) = response else {
        panic!("tool returned a notification response");
    };
    assert_eq!(output.text, "after-restart");
    managed.shutdown().await.unwrap();
}

#[tokio::test]
async fn malformed_oversized_and_crashing_extensions_fail_independently() {
    for input in ["malformed", "oversized", "crash"] {
        let extension = Extension::launch(fixture_config()).await.unwrap();
        let result = extension
            .request(
                ExtensionRequest::Tool(ToolCall {
                    name: "echo".into(),
                    arguments: json!({"text":input}),
                }),
                CancellationToken::new(),
            )
            .await;
        let Err(error) = result else {
            panic!("broken fixture returned a successful response");
        };
        if input == "crash" {
            assert!(matches!(error, ExtensionError::Exited { .. }));
        } else {
            assert!(matches!(error, ExtensionError::Protocol(_)));
        }
        let healthy = Extension::launch(fixture_config()).await.unwrap();
        assert_eq!(healthy.manifest().name, "fixture");
        healthy.shutdown().await.unwrap();
    }
}
