use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;

const TMUX_LOG_PROGRAM: &[u8] = include_bytes!("../../extensions/tmux-log/antex_ext_tmux_log.py");
const TMUX_LOG_DEFINITION: &[u8] = include_bytes!("../../extensions/tmux-log/extension.json");
const DIAGNOSTICS_PROGRAM: &[u8] =
    include_bytes!("../../extensions/diagnostics/antex_ext_diagnostics.py");
const DIAGNOSTICS_DEFINITION: &[u8] = include_bytes!("../../extensions/diagnostics/extension.json");
const WORKFLOWS_PROGRAM: &[u8] =
    include_bytes!("../../extensions/workflows/antex_ext_workflows.py");
const WORKFLOWS_DEFINITION: &[u8] = include_bytes!("../../extensions/workflows/extension.json");

struct Extension {
    name: &'static str,
    program_name: &'static str,
    program: &'static [u8],
    definition: &'static [u8],
}

const EXTENSIONS: &[Extension] = &[
    Extension {
        name: "diagnostics",
        program_name: "antex_ext_diagnostics.py",
        program: DIAGNOSTICS_PROGRAM,
        definition: DIAGNOSTICS_DEFINITION,
    },
    Extension {
        name: "tmux-log",
        program_name: "antex_ext_tmux_log.py",
        program: TMUX_LOG_PROGRAM,
        definition: TMUX_LOG_DEFINITION,
    },
    Extension {
        name: "workflows",
        program_name: "antex_ext_workflows.py",
        program: WORKFLOWS_PROGRAM,
        definition: WORKFLOWS_DEFINITION,
    },
];

pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    EXTENSIONS.iter().map(|extension| extension.name)
}

pub(crate) fn install(home: &Path, name: &str) -> io::Result<()> {
    let extension = EXTENSIONS
        .iter()
        .find(|extension| extension.name == name)
        .ok_or_else(|| io::Error::other(format!("unknown first-party extension: {name}")))?;
    let root = home.join("extensions");
    fs::create_dir_all(&root)?;
    let target = root.join(name);
    if target.exists() {
        return verify(&target, extension);
    }
    let staging = root.join(format!(".{name}-install-{}", std::process::id()));
    fs::create_dir(&staging)?;
    let result = (|| {
        write(
            &staging.join(extension.program_name),
            extension.program,
            true,
        )?;
        write(&staging.join("extension.json"), extension.definition, false)?;
        fs::rename(&staging, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn verify(directory: &Path, extension: &Extension) -> io::Result<()> {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir()
        || fs::read(directory.join(extension.program_name))? != extension.program
        || fs::read(directory.join("extension.json"))? != extension.definition
    {
        return Err(io::Error::other(
            "extension destination exists with different contents",
        ));
    }
    Ok(())
}

fn write(path: &Path, bytes: &[u8], executable: bool) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if executable { 0o700 } else { 0o600 });
    }
    #[cfg(not(unix))]
    let _ = executable;
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
#[path = "first_party_extensions_tests.rs"]
mod tests;
