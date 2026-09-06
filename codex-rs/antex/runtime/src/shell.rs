use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use antex_core::MAX_TEXT_BYTES;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use crate::PermissionProfile;
use crate::sandbox::Sandbox;

pub struct Shell {
    workspace: PathBuf,
    profile: PermissionProfile,
    sandbox: Sandbox,
    timeout: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShellResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Shell {
    pub fn new(workspace: &Path, profile: PermissionProfile) -> io::Result<Self> {
        Ok(Self {
            workspace: workspace.canonicalize()?,
            profile,
            sandbox: Sandbox::new("/usr/bin/bwrap".into()),
            timeout: Duration::from_secs(120),
        })
    }

    pub fn with_bubblewrap(mut self, program: PathBuf) -> Self {
        self.sandbox = Sandbox::new(program);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout.min(Duration::from_secs(900));
        self
    }

    pub async fn run(
        &self,
        script: &str,
        cancellation: CancellationToken,
    ) -> io::Result<ShellResult> {
        if script.len() > MAX_TEXT_BYTES || cancellation.is_cancelled() {
            return Err(io::Error::other("shell command is too large or cancelled"));
        }
        let temporary = tempfile::Builder::new().prefix("antex-shell-").tempdir()?;
        let temporary_path = temporary.path().canonicalize()?;
        let mut command = self
            .sandbox
            .command(&self.workspace, self.profile, script, &temporary_path)
            .await?;
        command
            .current_dir(&self.workspace)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
            .env("HOME", &temporary_path)
            .env("TMPDIR", &temporary_path)
            .env("LANG", "C.UTF-8")
            .env("TERM", "dumb")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if self.profile == PermissionProfile::Full
            && let Some(home) = std::env::var_os("HOME")
        {
            command.env("HOME", home);
        }
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn()?;
        let group = ProcessGroup(child.id().filter(|id| *id > 0));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing shell stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("missing shell stderr"))?;
        let execution = async {
            let (status, stdout, stderr) =
                tokio::join!(child.wait(), capture(stdout), capture(stderr));
            Ok(ShellResult {
                exit_code: status?.code(),
                stdout: stdout?,
                stderr: stderr?,
            })
        };
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted,"shell command cancelled")),
            result = tokio::time::timeout(self.timeout,execution) => result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut,"shell command timed out"))?,
        };
        drop(group);
        result
    }
}

struct ProcessGroup(Option<u32>);

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0.and_then(|pid| i32::try_from(pid).ok()) {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
}

async fn capture(mut stream: impl AsyncRead + Unpin) -> io::Result<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut truncated = false;
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let keep = count.min(MAX_TEXT_BYTES.saturating_sub(bytes.len()));
        truncated |= keep < count;
        bytes.extend_from_slice(&buffer[..keep]);
    }
    let mut output = String::from_utf8_lossy(&bytes).into_owned();
    if truncated || output.len() > MAX_TEXT_BYTES {
        let marker = "\n[output truncated]";
        let mut end = (MAX_TEXT_BYTES - marker.len()).min(output.len());
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
        output.push_str(marker);
    }
    Ok(output)
}
