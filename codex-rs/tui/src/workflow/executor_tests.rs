use pretty_assertions::assert_eq;
use tokio::sync::watch;

use super::*;

#[cfg(unix)]
#[tokio::test]
async fn nested_codex_jsonl_is_reduced_to_a_bounded_agent_result() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let fake_codex = temp.path().join("codex");
    tokio::fs::write(
        &fake_codex,
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"id":"msg","type":"agent_message","text":"fixed"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":12,"cached_input_tokens":8,"cache_write_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0}}'
"#,
    )
    .await
    .expect("write fake codex");
    let mut permissions = std::fs::metadata(&fake_codex)
        .expect("metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_codex, permissions).expect("chmod fake codex");
    let (_control_tx, control_rx) = watch::channel(WorkflowControl::Run);

    let result = execute_agent(
        AgentRequest {
            prompt: "Fix one issue".to_string(),
            model: None,
            cwd: None,
            timeout_seconds: Some(10),
        },
        /*default_model*/ None,
        /*default_cwd*/ None,
        /*default_timeout_seconds*/ None,
        &fake_codex,
        temp.path(),
        control_rx,
    )
    .await
    .expect("agent result");

    assert_eq!(
        result,
        AgentResult {
            success: true,
            exit_code: Some(0),
            message: "fixed".to_string(),
            error: String::new(),
            input_tokens: 12,
            cached_input_tokens: 8,
            output_tokens: 3,
        }
    );
}
