use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use antex_extension_protocol::Capability;
use antex_extension_protocol::CommandRun;
use antex_extension_protocol::Event;
use antex_extension_protocol::Initialize;
use antex_extension_protocol::MAX_FRAME_BYTES;
use antex_extension_protocol::Manifest;
use antex_extension_protocol::Method;
use antex_extension_protocol::Output;
use antex_extension_protocol::PROTOCOL_VERSION;
use antex_extension_protocol::ProtocolError;
use antex_extension_protocol::Request;
use antex_extension_protocol::Response;
use antex_extension_protocol::Version;
use futures::lock::Mutex;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const STDERR_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug)]
pub struct ExtensionConfig {
    pub name: String,
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub cwd: PathBuf,
    pub antex_version: String,
    pub session_id: String,
    pub capabilities: BTreeSet<Capability>,
    pub state: Value,
    pub timeout: Duration,
}

impl ExtensionConfig {
    pub fn new(name: impl Into<String>, program: impl Into<PathBuf>, cwd: PathBuf) -> Self {
        Self {
            name: name.into(),
            program: program.into(),
            arguments: Vec::new(),
            cwd,
            antex_version: env!("CARGO_PKG_VERSION").into(),
            session_id: String::new(),
            capabilities: BTreeSet::new(),
            state: Value::Null,
            timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("failed to start extension: {0}")]
    Start(#[source] std::io::Error),
    #[error("extension transport failed: {0}")]
    Transport(#[source] std::io::Error),
    #[error("extension protocol failed: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("extension timed out")]
    Timeout,
    #[error("extension request was cancelled")]
    Cancelled,
    #[error("agent actions require an explicit user command")]
    AgentOrigin,
    #[error("extension exited unexpectedly: {stderr}")]
    Exited { stderr: String },
}

pub enum ExtensionRequest {
    Tool(antex_extension_protocol::ToolCall),
    Command(CommandRun),
    Event(Event),
}

pub enum ExtensionResponse {
    Output(Output),
    Notified,
}

struct Transport {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub struct Extension {
    name: String,
    capabilities: BTreeSet<Capability>,
    manifest: Manifest,
    transport: Mutex<Option<Transport>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    next_id: AtomicU64,
    request_timeout: Duration,
}

impl Extension {
    pub async fn launch(config: ExtensionConfig) -> Result<Self, ExtensionError> {
        let mut command = Command::new(&config.program);
        command
            .args(&config.arguments)
            .current_dir(&config.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(ExtensionError::Start)?;
        let stdin = child.stdin.take().ok_or_else(|| {
            ExtensionError::Start(std::io::Error::other("extension stdin is unavailable"))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ExtensionError::Start(std::io::Error::other("extension stdout is unavailable"))
        })?;
        let child_stderr = child.stderr.take().ok_or_else(|| {
            ExtensionError::Start(std::io::Error::other("extension stderr is unavailable"))
        })?;
        let stderr = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(capture_stderr(child_stderr, Arc::clone(&stderr)));
        let extension = Self {
            name: config.name.clone(),
            capabilities: config.capabilities.clone(),
            manifest: Manifest {
                protocol_version: PROTOCOL_VERSION,
                name: config.name,
                tools: Vec::new(),
                commands: Vec::new(),
                events: BTreeSet::new(),
            },
            transport: Mutex::new(Some(Transport {
                child,
                stdin,
                stdout: BufReader::new(stdout),
            })),
            stderr,
            next_id: AtomicU64::new(1),
            request_timeout: config.timeout,
        };
        let initialize = Initialize {
            protocol_version: PROTOCOL_VERSION,
            antex_version: config.antex_version,
            session_id: config.session_id,
            cwd: config.cwd,
            capabilities: config.capabilities,
            state: config.state,
        };
        let value = extension
            .call(Method::Initialize, initialize, CancellationToken::new())
            .await?;
        let manifest: Manifest =
            serde_json::from_value(value).map_err(|_| ProtocolError::Encoding)?;
        manifest.validate(&extension.name, &extension.capabilities)?;
        Ok(Self {
            manifest,
            ..extension
        })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub async fn request(
        &self,
        request: ExtensionRequest,
        cancellation: CancellationToken,
    ) -> Result<ExtensionResponse, ExtensionError> {
        let (method, params, expects_output, user_command) = match request {
            ExtensionRequest::Tool(call) => {
                (Method::ToolCall, serde_json::to_value(call), true, false)
            }
            ExtensionRequest::Command(command) => (
                Method::CommandRun,
                serde_json::to_value(command),
                true,
                true,
            ),
            ExtensionRequest::Event(event) => (
                Method::EventNotify,
                serde_json::to_value(event),
                false,
                false,
            ),
        };
        let value = self
            .call(
                method,
                params.map_err(|_| ProtocolError::Encoding)?,
                cancellation,
            )
            .await?;
        if !expects_output {
            return Ok(ExtensionResponse::Notified);
        }
        let output: Output = serde_json::from_value(value).map_err(|_| ProtocolError::Encoding)?;
        output.validate(&self.capabilities)?;
        if !user_command
            && output
                .actions
                .iter()
                .any(|action| matches!(action, antex_extension_protocol::Action::Agent { .. }))
        {
            return Err(ExtensionError::AgentOrigin);
        }
        Ok(ExtensionResponse::Output(output))
    }

    pub async fn shutdown(&self) -> Result<(), ExtensionError> {
        let result = self
            .call(Method::Shutdown, json!({}), CancellationToken::new())
            .await;
        let mut guard = self.transport.lock().await;
        if let Some(mut transport) = guard.take() {
            let _ = transport.child.kill().await;
            let _ = transport.child.wait().await;
        }
        result.map(|_| ())
    }

    async fn call(
        &self,
        method: Method,
        params: impl Serialize,
        cancellation: CancellationToken,
    ) -> Result<Value, ExtensionError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let request = Request {
            jsonrpc: Version::V2,
            id: id.clone(),
            method,
            params: serde_json::to_value(params).map_err(|_| ProtocolError::Encoding)?,
        };
        let frame = antex_extension_protocol::encode(&request)?;
        let operation = async {
            let mut guard = self.transport.lock().await;
            let transport = guard.as_mut().ok_or_else(|| ExtensionError::Exited {
                stderr: String::new(),
            })?;
            transport
                .stdin
                .write_all(&frame)
                .await
                .map_err(ExtensionError::Transport)?;
            transport
                .stdin
                .flush()
                .await
                .map_err(ExtensionError::Transport)?;
            let response = read_frame(&mut transport.stdout).await?;
            if response.is_empty() {
                return Err(self.exited().await);
            }
            let response: Response = antex_extension_protocol::decode(&response)?;
            response.result(&id).map_err(ExtensionError::from)
        };
        let result = tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(ExtensionError::Cancelled),
            result = timeout(self.request_timeout, operation) => {
                result.map_err(|_| ExtensionError::Timeout)?
            }
        };
        if matches!(
            result,
            Err(ExtensionError::Cancelled | ExtensionError::Timeout)
        ) {
            self.terminate().await;
        }
        result
    }

    async fn exited(&self) -> ExtensionError {
        let stderr = self.stderr.lock().await;
        ExtensionError::Exited {
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        }
    }

    async fn terminate(&self) {
        let mut guard = self.transport.lock().await;
        if let Some(mut transport) = guard.take() {
            let _ = transport.child.kill().await;
            let _ = transport.child.wait().await;
        }
    }
}

async fn read_frame(reader: &mut BufReader<ChildStdout>) -> Result<Vec<u8>, ExtensionError> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(ExtensionError::Transport)?;
        if available.is_empty() {
            return Ok(frame);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if frame.len() + take > MAX_FRAME_BYTES + 1 {
            return Err(ProtocolError::Limit("frame").into());
        }
        frame.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            return Ok(frame);
        }
    }
}

async fn capture_stderr(stderr: tokio::process::ChildStderr, output: Arc<Mutex<Vec<u8>>>) {
    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let Ok(read) = reader.read_until(b'\n', &mut buffer).await else {
            break;
        };
        if read == 0 {
            break;
        }
        let mut output = output.lock().await;
        let remaining = STDERR_BYTES.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..buffer.len().min(remaining)]);
    }
}
