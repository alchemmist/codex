use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::watch;

use super::WorkflowControl;

const MAX_CAPTURE_BYTES: usize = 1_048_576;
const MAX_AGENT_MESSAGE_BYTES: usize = 65_536;

#[derive(Clone, Debug, Deserialize)]
pub(super) struct AgentRequest {
    pub(super) prompt: String,
    #[serde(default)]
    pub(super) model: Option<String>,
    #[serde(default)]
    pub(super) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(super) developer_instructions: Option<String>,
    #[serde(default)]
    pub(super) forbid_quality_graph_ignore: bool,
    #[serde(default)]
    pub(super) cwd: Option<String>,
    #[serde(default)]
    pub(super) timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct AgentResult {
    pub(super) success: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) message: String,
    pub(super) error: String,
    pub(super) input_tokens: i64,
    pub(super) cached_input_tokens: i64,
    pub(super) output_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct ShellResult {
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) truncated: bool,
}

pub(super) async fn execute_shell(
    argv: Vec<String>,
    requested_cwd: Option<String>,
    env: HashMap<String, String>,
    timeout_seconds: Option<u64>,
    workspace: &Path,
    control: watch::Receiver<WorkflowControl>,
) -> Result<ShellResult, String> {
    if *control.borrow() != WorkflowControl::Run {
        return Err("workflow interrupted".to_string());
    }
    if argv.is_empty() || argv.len() > 256 {
        return Err("shell argv must contain between 1 and 256 entries".to_string());
    }
    if argv.iter().any(|argument| argument.len() > 65_536) {
        return Err("shell argument exceeds 64 KiB".to_string());
    }
    if env.len() > 128
        || env
            .iter()
            .any(|(key, value)| key.len() > 256 || value.len() > 65_536)
    {
        return Err("shell environment exceeds workflow limits".to_string());
    }
    let cwd = resolve_cwd(workspace, requested_cwd.as_deref()).await?;
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to start `{}`: {err}", argv[0]))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture shell stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture shell stderr".to_string())?;
    let stdout_task = tokio::spawn(read_bounded(stdout, MAX_CAPTURE_BYTES));
    let stderr_task = tokio::spawn(read_bounded(stderr, MAX_CAPTURE_BYTES));
    let status = wait_for_child(
        &mut child,
        timeout_seconds.unwrap_or(/*default*/ 3_600),
        control,
    )
    .await?;
    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|err| format!("shell stdout reader failed: {err}"))?
        .map_err(|err| format!("failed to read shell stdout: {err}"))?;
    let (stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|err| format!("shell stderr reader failed: {err}"))?
        .map_err(|err| format!("failed to read shell stderr: {err}"))?;
    Ok(ShellResult {
        exit_code: status.code(),
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
    })
}

pub(super) async fn execute_agent(
    mut request: AgentRequest,
    default_model: Option<String>,
    default_cwd: Option<String>,
    default_timeout_seconds: Option<u64>,
    codex_exe: &Path,
    workspace: &Path,
    control: watch::Receiver<WorkflowControl>,
) -> Result<AgentResult, String> {
    if *control.borrow() != WorkflowControl::Run {
        return Err("workflow interrupted".to_string());
    }
    if request.prompt.trim().is_empty() || request.prompt.len() > 1_048_576 {
        return Err("agent prompt must contain between 1 byte and 1 MiB".to_string());
    }
    if request
        .developer_instructions
        .as_ref()
        .is_some_and(|instructions| instructions.len() > 65_536)
    {
        return Err("agent developer instructions exceed 64 KiB".to_string());
    }
    request.model = request.model.or(default_model);
    request.cwd = request.cwd.or(default_cwd);
    request.timeout_seconds = request.timeout_seconds.or(default_timeout_seconds);
    let cwd = resolve_cwd(workspace, request.cwd.as_deref()).await?;
    let mut command = Command::new(codex_exe);
    command
        .arg("exec")
        .arg("--json")
        .arg("--ephemeral")
        .arg("--skip-git-repo-check")
        .arg("--sandbox")
        .arg("workspace-write")
        .arg("--cd")
        .arg(&cwd);
    if let Some(model) = &request.model
        && !model.trim().is_empty()
    {
        command.arg("--model").arg(model);
    }
    if let Some(reasoning_effort) = request
        .reasoning_effort
        .as_deref()
        .filter(|effort| !effort.trim().is_empty())
    {
        command.arg("-c").arg(format!(
            "model_reasoning_effort={}",
            toml::Value::String(reasoning_effort.to_string())
        ));
    }
    if let Some(developer_instructions) = request
        .developer_instructions
        .as_deref()
        .filter(|instructions| !instructions.trim().is_empty())
    {
        command.arg("-c").arg(format!(
            "developer_instructions={}",
            toml::Value::String(developer_instructions.to_string())
        ));
    }
    command
        .arg("-")
        .env("CODEX_WORKFLOW_DEPTH", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if request.forbid_quality_graph_ignore {
        command.env("CODEX_WORKFLOW_FORBID_QUALITY_GRAPH_IGNORE", "1");
    }
    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to start nested Codex agent: {err}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open nested Codex stdin".to_string())?;
    stdin
        .write_all(request.prompt.as_bytes())
        .await
        .map_err(|err| format!("failed to send nested Codex prompt: {err}"))?;
    drop(stdin);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture nested Codex stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture nested Codex stderr".to_string())?;
    let stdout_task = tokio::spawn(read_agent_events(stdout));
    let stderr_task = tokio::spawn(read_bounded(stderr, MAX_CAPTURE_BYTES));
    let status = wait_for_child(
        &mut child,
        request.timeout_seconds.unwrap_or(/*default*/ 3_600),
        control,
    )
    .await?;
    let parsed = stdout_task
        .await
        .map_err(|err| format!("nested Codex event reader failed: {err}"))?
        .map_err(|err| format!("failed to read nested Codex events: {err}"))?;
    let (stderr, _) = stderr_task
        .await
        .map_err(|err| format!("nested Codex stderr reader failed: {err}"))?
        .map_err(|err| format!("failed to read nested Codex stderr: {err}"))?;
    Ok(AgentResult {
        success: status.success() && parsed.turn_error.is_none(),
        exit_code: status.code(),
        message: parsed.message,
        error: parsed.turn_error.unwrap_or(stderr),
        input_tokens: parsed.input_tokens,
        cached_input_tokens: parsed.cached_input_tokens,
        output_tokens: parsed.output_tokens,
    })
}

async fn resolve_cwd(workspace: &Path, requested: Option<&str>) -> Result<PathBuf, String> {
    let workspace = tokio::fs::canonicalize(workspace)
        .await
        .map_err(|err| format!("failed to resolve workflow workspace: {err}"))?;
    let candidate = match requested {
        Some(requested) => {
            let requested = PathBuf::from(requested);
            if requested.is_absolute() {
                requested
            } else {
                workspace.join(requested)
            }
        }
        None => workspace.clone(),
    };
    let candidate = tokio::fs::canonicalize(&candidate).await.map_err(|err| {
        format!(
            "failed to resolve workflow cwd {}: {err}",
            candidate.display()
        )
    })?;
    if !candidate.starts_with(&workspace) {
        return Err(format!(
            "workflow cwd {} is outside workspace {}",
            candidate.display(),
            workspace.display()
        ));
    }
    Ok(candidate)
}

async fn wait_for_child(
    child: &mut Child,
    timeout_seconds: u64,
    mut control: watch::Receiver<WorkflowControl>,
) -> Result<ExitStatus, String> {
    if *control.borrow() != WorkflowControl::Run {
        let _ = child.kill().await;
        return Err("workflow interrupted".to_string());
    }
    let timeout = tokio::time::sleep(Duration::from_secs(timeout_seconds.clamp(1, 86_400)));
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            status = child.wait() => {
                return status.map_err(|err| format!("failed waiting for child process: {err}"));
            }
            _ = &mut timeout => {
                let _ = child.kill().await;
                return Err(format!("workflow action timed out after {timeout_seconds}s"));
            }
            changed = control.changed() => {
                if changed.is_err() || *control.borrow() != WorkflowControl::Run {
                    let _ = child.kill().await;
                    return Err("workflow interrupted".to_string());
                }
            }
        }
    }
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> std::io::Result<(String, bool)> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8_192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

#[derive(Default)]
struct ParsedAgentEvents {
    message: String,
    turn_error: Option<String>,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
}

async fn read_agent_events(reader: impl AsyncRead + Unpin) -> std::io::Result<ParsedAgentEvents> {
    let mut parsed = ParsedAgentEvents::default();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("item.completed")
                if event
                    .pointer("/item/type")
                    .and_then(serde_json::Value::as_str)
                    == Some("agent_message") =>
            {
                if let Some(message) = event
                    .pointer("/item/text")
                    .and_then(serde_json::Value::as_str)
                {
                    parsed.message = truncate_string(message, MAX_AGENT_MESSAGE_BYTES);
                }
            }
            Some("turn.completed") => {
                parsed.input_tokens = event
                    .pointer("/usage/input_tokens")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or_default();
                parsed.cached_input_tokens = event
                    .pointer("/usage/cached_input_tokens")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or_default();
                parsed.output_tokens = event
                    .pointer("/usage/output_tokens")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or_default();
            }
            Some("turn.failed") | Some("error") => {
                parsed.turn_error = event
                    .pointer("/error/message")
                    .or_else(|| event.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(|message| truncate_string(message, MAX_AGENT_MESSAGE_BYTES));
            }
            _ => {}
        }
    }
    Ok(parsed)
}

fn truncate_string(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}\n…truncated…", &value[..end])
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
