use std::collections::BTreeMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use antex_core::ErrorKind;
use antex_core::ProviderError;
use serde::Deserialize;
use serde::Serialize;

use crate::wire::error;

const MAX_CREDENTIAL_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: String,
    pub expires_at: u64,
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct Accounts {
    pub active: Option<String>,
    pub entries: BTreeMap<String, Tokens>,
}

pub(crate) struct Store {
    home: PathBuf,
    directory: PathBuf,
}

impl Store {
    pub fn new(home: &Path) -> Self {
        Self {
            home: home.to_owned(),
            directory: home.join("providers/openai"),
        }
    }

    pub async fn lock(&self) -> Result<File, ProviderError> {
        let directory = self.directory.clone();
        let home = self.home.clone();
        tokio::task::spawn_blocking(move || {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&home).map_err(storage_error)?;
            if std::fs::symlink_metadata(&home)
                .map_err(storage_error)?
                .file_type()
                .is_symlink()
            {
                return Err(error(
                    ErrorKind::Authentication,
                    "Antex home must not be a symlink",
                ));
            }
            let providers = directory
                .parent()
                .ok_or_else(|| error(ErrorKind::Authentication, "invalid credential directory"))?;
            for path in [providers, directory.as_path()] {
                match std::fs::create_dir(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(storage_error(error)),
                }
                let metadata = std::fs::symlink_metadata(path).map_err(storage_error)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(error(
                        ErrorKind::Authentication,
                        "credential directories must not be symlinks",
                    ));
                }
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = std::fs::symlink_metadata(&directory).map_err(storage_error)?;
                if metadata.file_type().is_symlink() {
                    return Err(error(
                        ErrorKind::Authentication,
                        "credential directory must not be a symlink",
                    ));
                }
                std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                    .map_err(storage_error)?;
            }
            let file = open_private(&directory.join("auth.lock"), OpenMode::Create)?;
            file.lock().map_err(storage_error)?;
            Ok(file)
        })
        .await
        .map_err(|_| error(ErrorKind::Authentication, "credential lock task failed"))?
    }

    pub fn load(&self) -> Result<Accounts, ProviderError> {
        let path = self.directory.join("auth.json");
        if !path.try_exists().map_err(storage_error)? {
            return Ok(Accounts::default());
        }
        let file = open_private(&path, OpenMode::Existing)?;
        let mut bytes = Vec::new();
        file.take(MAX_CREDENTIAL_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(storage_error)?;
        if bytes.len() as u64 > MAX_CREDENTIAL_BYTES {
            return Err(error(
                ErrorKind::Authentication,
                "credential store exceeds its byte budget",
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| error(ErrorKind::Authentication, "credential store is malformed"))
    }

    pub fn save(&self, accounts: &Accounts) -> Result<(), ProviderError> {
        let bytes = serde_json::to_vec(accounts)
            .map_err(|_| error(ErrorKind::Authentication, "cannot encode credentials"))?;
        if bytes.len() as u64 > MAX_CREDENTIAL_BYTES {
            return Err(error(
                ErrorKind::Authentication,
                "credential store exceeds its byte budget",
            ));
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.directory).map_err(storage_error)?;
        temporary.write_all(&bytes).map_err(storage_error)?;
        temporary.as_file().sync_all().map_err(storage_error)?;
        temporary
            .persist(self.directory.join("auth.json"))
            .map_err(|_| error(ErrorKind::Authentication, "cannot commit credential store"))?;
        File::open(&self.directory)
            .and_then(|file| file.sync_all())
            .map_err(storage_error)
    }
}

enum OpenMode {
    Existing,
    Create,
}

fn open_private(path: &Path, mode: OpenMode) -> Result<File, ProviderError> {
    let mut options = OpenOptions::new();
    options.read(true);
    if let OpenMode::Create = mode {
        options.create(true).write(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(storage_error)?;
    let metadata = file.metadata().map_err(storage_error)?;
    if !metadata.is_file() {
        return Err(error(
            ErrorKind::Authentication,
            "credential store must be a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(error(
                ErrorKind::Authentication,
                "credential store must be owned by the current user with mode 0600",
            ));
        }
    }
    Ok(file)
}

fn storage_error(_error: std::io::Error) -> ProviderError {
    error(
        ErrorKind::Authentication,
        "cannot access private credential store",
    )
}
