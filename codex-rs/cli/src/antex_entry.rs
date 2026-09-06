use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;

pub(super) fn is_antex() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(OsStr::to_owned))
        .is_some_and(|name| name == "antex")
}

pub(super) fn home() -> anyhow::Result<PathBuf> {
    let user_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("cannot locate the user home directory")?
        .canonicalize()
        .context("cannot resolve the user home directory")?;
    let codex_home = user_home.join(".codex");
    let codex_home = codex_home.canonicalize().unwrap_or(codex_home);
    let antex_home = match std::env::var_os("ANTEX_HOME").filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value)
            .canonicalize()
            .context("ANTEX_HOME must point to an existing directory")?,
        None => {
            let path = user_home.join(".antex");
            match path.canonicalize() {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => path,
                Err(error) => return Err(error).context("cannot resolve the Antex home"),
            }
        }
    };
    anyhow::ensure!(
        !antex_home.starts_with(&codex_home),
        "Antex home must not be inside the legacy .codex directory"
    );
    Ok(antex_home)
}

pub(super) fn prepare() -> anyhow::Result<()> {
    let antex_home = home()?;
    std::fs::create_dir_all(&antex_home).context("cannot create the Antex home directory")?;
    anyhow::ensure!(antex_home.is_dir(), "ANTEX_HOME must point to a directory");

    if std::env::var_os("CODEX_HOME").as_deref() == Some(antex_home.as_os_str())
        && std::env::var_os("CODEX_SQLITE_HOME").as_deref() == Some(antex_home.as_os_str())
    {
        return Ok(());
    }

    let executable = std::env::current_exe().context("cannot locate the Antex executable")?;
    let mut command = Command::new(executable);
    command
        .args(std::env::args_os().skip(1))
        .env("ANTEX_HOME", &antex_home)
        .env("CODEX_HOME", &antex_home)
        .env("CODEX_SQLITE_HOME", &antex_home);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        if let Some(arg0) = std::env::args_os().next() {
            command.arg0(arg0);
        }
        Err(command.exec()).context("cannot start the Antex runtime")
    }
    #[cfg(not(unix))]
    {
        let status = command.status().context("cannot start the Antex runtime")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
