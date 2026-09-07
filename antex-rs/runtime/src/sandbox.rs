use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;
#[cfg(target_os = "linux")]
use tokio::sync::OnceCell;

use crate::PermissionProfile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxWorkspace {
    Hidden,
    ReadOnly,
    ReadWrite,
}

pub(crate) struct SandboxProgram<'a> {
    pub workspace: &'a Path,
    pub access: SandboxWorkspace,
    pub network: bool,
    pub program: &'a Path,
    pub arguments: &'a [OsString],
    pub temporary: &'a Path,
    pub read_roots: &'a [PathBuf],
}

pub(crate) struct Sandbox {
    #[cfg(target_os = "linux")]
    program: PathBuf,
    #[cfg(target_os = "linux")]
    verified: OnceCell<()>,
}

impl Sandbox {
    pub fn new(program: PathBuf) -> Self {
        #[cfg(not(target_os = "linux"))]
        let _ = program;
        Self {
            #[cfg(target_os = "linux")]
            program,
            #[cfg(target_os = "linux")]
            verified: OnceCell::new(),
        }
    }

    pub async fn command(
        &self,
        workspace: &Path,
        profile: PermissionProfile,
        script: &str,
        temporary: &Path,
        read_roots: &[PathBuf],
    ) -> io::Result<Command> {
        if profile == PermissionProfile::Full {
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg(script).stdin(Stdio::null());
            return Ok(command);
        }
        let access = match profile {
            PermissionProfile::ReadOnly => SandboxWorkspace::ReadOnly,
            PermissionProfile::Workspace => SandboxWorkspace::ReadWrite,
            PermissionProfile::Full => unreachable!(),
        };
        let arguments = [OsString::from("-c"), OsString::from(script)];
        self.program_command(SandboxProgram {
            workspace,
            access,
            network: false,
            program: Path::new("/bin/sh"),
            arguments: &arguments,
            temporary,
            read_roots,
        })
        .await
    }

    pub async fn program_command(&self, spec: SandboxProgram<'_>) -> io::Result<Command> {
        let SandboxProgram {
            workspace,
            access,
            network,
            program,
            arguments,
            temporary,
            read_roots,
        } = spec;
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
            if network {
                command.arg("--share-net");
            }
            for root in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"] {
                if Path::new(root).exists() {
                    command.arg("--ro-bind").arg(root).arg(root);
                }
            }
            command.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
            command.arg("--bind").arg(temporary).arg(temporary);
            for root in read_roots {
                command.arg("--ro-bind").arg(root).arg(root);
            }
            match access {
                SandboxWorkspace::Hidden => {}
                SandboxWorkspace::ReadOnly => {
                    command.arg("--ro-bind").arg(workspace).arg(workspace);
                }
                SandboxWorkspace::ReadWrite => {
                    command.arg("--bind").arg(workspace).arg(workspace);
                }
            }
            attach_seccomp(&mut command, crate::seccomp::filter(!network)?)?;
            let working_directory = match access {
                SandboxWorkspace::Hidden => temporary,
                SandboxWorkspace::ReadOnly | SandboxWorkspace::ReadWrite => workspace,
            };
            command
                .arg("--chdir")
                .arg(working_directory)
                .arg("--")
                .arg(program)
                .args(arguments);
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
            ]
            .into_iter()
            .chain((access != SandboxWorkspace::Hidden).then_some(workspace))
            .chain(read_roots.iter().map(PathBuf::as_path))
            {
                let root = root
                    .to_str()
                    .ok_or_else(|| io::Error::other("sandbox path is not UTF-8"))?;
                policy.push_str(&format!(
                    "(allow file-read* file-map-executable (subpath {}))",
                    serde_json::to_string(root)?
                ));
            }
            for root in std::iter::once(temporary)
                .chain((access == SandboxWorkspace::ReadWrite).then_some(workspace))
            {
                let root = root
                    .to_str()
                    .ok_or_else(|| io::Error::other("sandbox path is not UTF-8"))?;
                policy.push_str(&format!(
                    "(allow file-read* file-write* file-map-executable (subpath {}))",
                    serde_json::to_string(root)?
                ));
            }
            let mut command = Command::new("/usr/bin/sandbox-exec");
            if network {
                policy.push_str("(allow network*)");
            }
            command.arg("-p").arg(&policy).arg(program).args(arguments);
            Ok(command)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "sandboxing is supported only on Linux and macOS",
        ))
    }
}

#[cfg(target_os = "linux")]
fn attach_seccomp(command: &mut Command, filter: std::fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    const FILTER_FD: i32 = 3;
    let source = filter.as_raw_fd();
    command.args(["--seccomp", "3"]);
    unsafe {
        command.as_std_mut().pre_exec(move || {
            if libc::dup2(source, FILTER_FD) == -1 {
                return Err(io::Error::last_os_error());
            }
            if libc::fcntl(FILTER_FD, libc::F_SETFD, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            let _ = &filter;
            Ok(())
        });
    }
    Ok(())
}
