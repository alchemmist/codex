use std::io;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::PermissionProfile;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub model: Option<String>,
    pub model_reasoning_effort: Option<String>,
    pub permissions: PermissionProfile,
    pub shell_timeout_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: None,
            model_reasoning_effort: None,
            permissions: PermissionProfile::Workspace,
            shell_timeout_seconds: 120,
        }
    }
}

pub struct LoadedConfig {
    pub config: Config,
    pub warnings: Vec<String>,
}

impl Config {
    pub fn load(home: &Path) -> io::Result<LoadedConfig> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = match options.open(home.join("config.toml")) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(LoadedConfig {
                    config: Self::default(),
                    warnings: Vec::new(),
                });
            }
            Err(error) => return Err(error),
        };
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("configuration must be a regular file"));
        }
        let mut text = String::new();
        file.take(64 * 1024 + 1).read_to_string(&mut text)?;
        if text.len() > 64 * 1024 {
            return Err(io::Error::other("configuration exceeds its byte budget"));
        }
        let table: toml::Table = toml::from_str(&text)
            .map_err(|_| io::Error::other("invalid Antex configuration document"))?;
        let config: Self = toml::from_str(&text)
            .map_err(|_| io::Error::other("invalid Antex configuration field types"))?;
        if config.model.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
        }) || config.model_reasoning_effort.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > 64 || value.chars().any(char::is_control)
        }) || !(1..=900).contains(&config.shell_timeout_seconds)
        {
            return Err(io::Error::other(
                "Antex configuration contains invalid values",
            ));
        }
        let known = [
            "model",
            "model_reasoning_effort",
            "permissions",
            "shell_timeout_seconds",
        ];
        let warnings = table
            .keys()
            .filter(|key| !known.contains(&key.as_str()))
            .take(64)
            .map(|key| {
                format!(
                    "unsupported configuration key: {}",
                    key.chars()
                        .filter(|character| !character.is_control())
                        .take(128)
                        .collect::<String>()
                )
            })
            .collect();
        Ok(LoadedConfig { config, warnings })
    }
}
