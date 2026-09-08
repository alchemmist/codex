use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use cap_std::fs::OpenOptions;

use crate::PermissionProfile;

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

pub struct WorkspaceFiles {
    workspace: PathBuf,
    root: PathBuf,
    directory: Dir,
    profile: PermissionProfile,
    read_roots: Vec<(PathBuf, Dir)>,
}

impl WorkspaceFiles {
    pub fn new(workspace: &Path, profile: PermissionProfile) -> io::Result<Self> {
        let workspace = workspace.canonicalize()?;
        let root = match profile {
            PermissionProfile::ReadOnly | PermissionProfile::Workspace => workspace.clone(),
            PermissionProfile::Full => workspace
                .ancestors()
                .last()
                .ok_or_else(|| io::Error::other("workspace has no root"))?
                .to_owned(),
        };
        let directory = Dir::open_ambient_dir(&root, ambient_authority())?;
        Ok(Self {
            workspace,
            root,
            directory,
            profile,
            read_roots: Vec::new(),
        })
    }

    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        if path.is_empty() || path.len() > 4096 {
            return Err(io::Error::other("invalid file path length"));
        }
        let absolute = self.workspace.join(path);
        let (directory, path) = match absolute.strip_prefix(&self.root) {
            Ok(path) => (&self.directory, path),
            Err(_) => self
                .read_roots
                .iter()
                .find_map(|(root, directory)| {
                    absolute
                        .strip_prefix(root)
                        .ok()
                        .map(|path| (directory, path))
                })
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "path is outside the allowed read roots",
                    )
                })?,
        };
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = directory.open_with(path, &options)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("read requires a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(io::Error::other("file exceeds the read budget"));
        }
        Ok(bytes)
    }

    pub fn with_read_roots(mut self, roots: &[PathBuf]) -> io::Result<Self> {
        if roots.len() > 16 {
            return Err(io::Error::other("too many additional read roots"));
        }
        self.read_roots = roots
            .iter()
            .map(|root| {
                let root = root.canonicalize()?;
                let directory = Dir::open_ambient_dir(&root, ambient_authority())?;
                Ok((root, directory))
            })
            .collect::<io::Result<_>>()?;
        Ok(self)
    }

    pub fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        if self.profile == PermissionProfile::ReadOnly {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only profile forbids writes",
            ));
        }
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(io::Error::other("file exceeds the write budget"));
        }
        let path = self.relative(path)?;
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        let directory = self.directory.open_dir(if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        })?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("write requires a file name"))?;
        let permissions = match directory.symlink_metadata(name) {
            Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
            Ok(_) => {
                return Err(io::Error::other(
                    "write target must be a regular file, not a symlink",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let temporary_name = format!(".antex-write-{}", uuid::Uuid::new_v4());
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut temporary = directory.open_with(&temporary_name, &options)?;
        let result = (|| {
            temporary.write_all(bytes)?;
            if let Some(permissions) = permissions {
                temporary.set_permissions(permissions)?;
            }
            temporary.sync_all()?;
            directory.rename(&temporary_name, &directory, name)
        })();
        if result.is_err() {
            let _ = directory.remove_file(&temporary_name);
        }
        result
    }

    pub fn edit(&self, path: &str, old_text: &str, new_text: &str) -> io::Result<()> {
        let bytes = self.read(path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| io::Error::other("edit requires UTF-8 text"))?;
        let updated = crate::edit::replace_unique(text, old_text, new_text)?;
        self.write(path, updated.as_bytes())
    }

    fn relative(&self, path: &str) -> io::Result<PathBuf> {
        if path.len() > 4096 || path.is_empty() {
            return Err(io::Error::other("invalid file path length"));
        }
        let path = self.workspace.join(path);
        path.strip_prefix(&self.root)
            .map(Path::to_owned)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "path is outside the allowed root",
                )
            })
    }
}
