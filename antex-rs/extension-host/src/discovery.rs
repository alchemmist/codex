use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use antex_extension_protocol::Capability;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredExtension {
    pub name: String,
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub capabilities: BTreeSet<Capability>,
    pub project: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Definition {
    name: String,
    program: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    capabilities: BTreeSet<Capability>,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("failed to read extension directory: {0}")]
    Read(#[source] std::io::Error),
    #[error("invalid extension definition {path}: {source}")]
    Definition {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("extension program must be a regular executable file inside its directory: {0}")]
    Program(PathBuf),
    #[error("duplicate extension name: {0}")]
    Duplicate(String),
}

pub fn discover(
    home: &Path,
    workspace: &Path,
    project_trusted: bool,
) -> Result<Vec<DiscoveredExtension>, DiscoveryError> {
    let mut discovered = Vec::new();
    read_directory(&home.join("extensions"), false, &mut discovered)?;
    if project_trusted {
        read_directory(&workspace.join(".antex/extensions"), true, &mut discovered)?;
    }
    discovered.sort_by(|left, right| left.name.cmp(&right.name));
    for pair in discovered.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(DiscoveryError::Duplicate(pair[0].name.clone()));
        }
    }
    Ok(discovered)
}

fn read_directory(
    root: &Path,
    project: bool,
    discovered: &mut Vec<DiscoveredExtension>,
) -> Result<(), DiscoveryError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(DiscoveryError::Read(error)),
    };
    for entry in entries {
        let entry = entry.map_err(DiscoveryError::Read)?;
        let directory = entry.path();
        if !entry.file_type().map_err(DiscoveryError::Read)?.is_dir() {
            continue;
        }
        let definition_path = directory.join("extension.json");
        let bytes = match fs::read(&definition_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(DiscoveryError::Read(error)),
        };
        let definition: Definition =
            serde_json::from_slice(&bytes).map_err(|source| DiscoveryError::Definition {
                path: definition_path,
                source,
            })?;
        let program = directory.join(&definition.program);
        let canonical_directory = directory.canonicalize().map_err(DiscoveryError::Read)?;
        let canonical_program = program.canonicalize().map_err(DiscoveryError::Read)?;
        let metadata = canonical_program.metadata().map_err(DiscoveryError::Read)?;
        if !canonical_program.starts_with(&canonical_directory) || !metadata.is_file() {
            return Err(DiscoveryError::Program(program));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(DiscoveryError::Program(program));
            }
        }
        discovered.push(DiscoveredExtension {
            name: definition.name,
            program: canonical_program,
            arguments: definition.arguments,
            capabilities: definition.capabilities,
            project,
        });
    }
    Ok(())
}
