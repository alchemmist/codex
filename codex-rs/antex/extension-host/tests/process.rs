use std::path::PathBuf;
use std::time::Duration;

use antex_extension_host::Extension;
use antex_extension_host::ExtensionConfig;
use antex_extension_host::ExtensionError;
use antex_extension_host::ExtensionRequest;
use antex_extension_host::ExtensionResponse;
use antex_extension_protocol::Capability;
use antex_extension_protocol::CommandRun;
use antex_extension_protocol::ToolCall;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn fixture_config() -> ExtensionConfig {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance.py");
    let mut config =
        ExtensionConfig::new("fixture", "/usr/bin/env", fixture.parent().unwrap().into());
    config.arguments = vec!["python3".into(), fixture.into_os_string()];
    config.session_id = "session-1".into();
    config.timeout = Duration::from_secs(2);
    config
}

#[tokio::test]
async fn python_fixture_registers_and_serves_tools_and_commands() {
    let extension = Extension::launch(fixture_config()).await.unwrap();
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
