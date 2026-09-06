use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;
use tokio::sync::OnceCell;

use crate::PermissionProfile;

pub(crate) struct Sandbox {
    program: PathBuf,
    verified: OnceCell<()>,
}

impl Sandbox {
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            verified: OnceCell::new(),
        }
    }

    pub async fn command(
        &self,
        workspace: &Path,
        profile: PermissionProfile,
        script: &str,
        temporary: &Path,
    ) -> io::Result<Command> {
        if profile == PermissionProfile::Full {
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg(script).stdin(Stdio::null());
            return Ok(command);
        }
        #[cfg(target_os = "linux")]
        {
            self.verified.get_or_try_init(|| async {
                let program = self.program.canonicalize()?;
                if program.starts_with(workspace) {
                    return Err(io::Error::other("sandbox program must be outside the workspace"));
                }
                let output = tokio::time::timeout(std::time::Duration::from_secs(5), Command::new(&program).arg("--version").output()).await
                    .map_err(|_| io::Error::other("sandbox version check timed out"))??;
                let version = String::from_utf8_lossy(&output.stdout);
                let components: Vec<u64> = version.split_whitespace().nth(1).unwrap_or_default().split('.').filter_map(|part| part.parse().ok()).collect();
                if !output.status.success() || components.len() < 2 || (components[0],components[1]) < (0,12) {
                    return Err(io::Error::other("bubblewrap 0.12 or newer is required; refusing an unsandboxed fallback"));
                }
                Ok(())
            }).await?;
            let mut command = Command::new(&self.program);
            command.args([
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--cap-drop",
                "ALL",
            ]);
            for root in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"] {
                if Path::new(root).exists() {
                    command.arg("--ro-bind").arg(root).arg(root);
                }
            }
            command.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
            command.arg("--bind").arg(temporary).arg(temporary);
            let binding = match profile {
                PermissionProfile::ReadOnly => "--ro-bind",
                PermissionProfile::Workspace => "--bind",
                PermissionProfile::Full => unreachable!(),
            };
            command.arg(binding).arg(workspace).arg(workspace);
            command
                .args(["--seccomp", "0"])
                .stdin(Stdio::from(crate::seccomp::filter()?));
            command
                .arg("--chdir")
                .arg(workspace)
                .args(["--", "/bin/sh", "-c", script]);
            Ok(command)
        }
        #[cfg(target_os = "macos")]
        {
            let mut policy = String::from(
                "(version 1)(deny default)(allow process-exec)(allow process-fork)(allow signal (target same-sandbox))(allow process-info* (target same-sandbox))(allow sysctl-read)(allow file-read-metadata)(allow mach-lookup (global-name \"com.apple.system.opendirectoryd.libinfo\"))(allow file-read* file-write-data (literal \"/dev/null\"))(allow file-read* (subpath \"/dev/fd\"))(allow file-write-data (literal \"/dev/fd/1\") (literal \"/dev/fd/2\"))",
            );
            for root in [
                Path::new("/System"),
                Path::new("/usr"),
                Path::new("/bin"),
                Path::new("/sbin"),
                Path::new("/private/etc"),
                Path::new("/Library/Apple"),
                workspace,
            ] {
                let root = root
                    .to_str()
                    .ok_or_else(|| io::Error::other("sandbox path is not UTF-8"))?;
                policy.push_str(&format!(
                    "(allow file-read* (subpath {}))",
                    serde_json::to_string(root)?
                ));
            }
            for root in std::iter::once(temporary)
                .chain((profile == PermissionProfile::Workspace).then_some(workspace))
            {
                let root = root
                    .to_str()
                    .ok_or_else(|| io::Error::other("sandbox path is not UTF-8"))?;
                policy.push_str(&format!(
                    "(allow file-read* file-write* (subpath {}))",
                    serde_json::to_string(root)?
                ));
            }
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command
                .args(["-p", &policy, "/bin/sh", "-c", script])
                .stdin(Stdio::null());
            Ok(command)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "sandboxing is supported only on Linux and macOS",
        ))
    }
}
