use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use tokio::process::Command;

use crate::sandbox::Sandbox;
use crate::sandbox::SandboxProgram;
use crate::sandbox::SandboxWorkspace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionWorkspace {
    Hidden,
    ReadOnly,
    ReadWrite,
}

pub struct ExtensionSandbox {
    workspace: PathBuf,
    temporary: PathBuf,
    sandbox: Sandbox,
    read_roots: Vec<PathBuf>,
}

impl ExtensionSandbox {
    pub fn new(
        home: &Path,
        workspace: &Path,
        bubblewrap: PathBuf,
        read_roots: &[PathBuf],
    ) -> io::Result<Self> {
        let workspace = workspace.canonicalize()?;
        let temporary = home.join("tmp/extensions");
        std::fs::create_dir_all(&temporary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o700))?;
        }
        let temporary = temporary.canonicalize()?;
        let read_roots = read_roots
            .iter()
            .map(|root| root.canonicalize())
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            workspace,
            temporary,
            sandbox: Sandbox::new(bubblewrap),
            read_roots,
        })
    }

    pub async fn command(
        &self,
        program: &Path,
        arguments: &[OsString],
        workspace: ExtensionWorkspace,
        network: bool,
    ) -> io::Result<Command> {
        let program = program.canonicalize()?;
        let program_root = program
            .parent()
            .ok_or_else(|| io::Error::other("extension program has no parent directory"))?;
        let mut read_roots = self.read_roots.clone();
        if !read_roots.iter().any(|root| program.starts_with(root)) {
            read_roots.push(program_root.to_path_buf());
        }
        if read_roots.len() > 17 {
            return Err(io::Error::other("too many extension read roots"));
        }
        let workspace = match workspace {
            ExtensionWorkspace::Hidden => SandboxWorkspace::Hidden,
            ExtensionWorkspace::ReadOnly => SandboxWorkspace::ReadOnly,
            ExtensionWorkspace::ReadWrite => SandboxWorkspace::ReadWrite,
        };
        let mut command = self
            .sandbox
            .program_command(SandboxProgram {
                workspace: &self.workspace,
                access: workspace,
                network,
                program: &program,
                arguments,
                temporary: &self.temporary,
                read_roots: &read_roots,
            })
            .await?;
        command
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
            .env("HOME", &self.temporary)
            .env("TMPDIR", &self.temporary)
            .env("LANG", "C.UTF-8")
            .env("TERM", "dumb");
        Ok(command)
    }
}
