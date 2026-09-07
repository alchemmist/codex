use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use crate::StartupMascotSkin;
use crate::editor_types::VimModeStart;
use crate::keymap::RuntimeKeymap;
use crate::keymap_config::TuiKeymap;

#[derive(Clone)]
pub struct Settings {
    pub(crate) keymap: Arc<RuntimeKeymap>,
    pub(crate) vim_mode: bool,
    pub(crate) vim_start: VimModeStart,
    pub(crate) show_vim_mode: bool,
    pub(crate) animations: bool,
    pub(crate) title: String,
    pub(crate) mascot: StartupMascotSkin,
    pub(crate) theme: Option<String>,
    pub(crate) home: Option<PathBuf>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(default)]
struct RawSettings {
    animations: bool,
    vim_mode_default: bool,
    vim_mode_start: VimModeStart,
    show_vim_mode_indicator: bool,
    startup_panel: StartupSettings,
    keymap: TuiKeymap,
    theme: Option<String>,
}

impl Default for RawSettings {
    fn default() -> Self {
        Self {
            animations: true,
            vim_mode_default: false,
            vim_mode_start: VimModeStart::Normal,
            show_vim_mode_indicator: false,
            startup_panel: StartupSettings::default(),
            keymap: TuiKeymap::default(),
            theme: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct StartupSettings {
    title: String,
    mascot_skin: StartupMascotSkin,
}

impl Default for StartupSettings {
    fn default() -> Self {
        Self {
            title: "Antex".into(),
            mascot_skin: StartupMascotSkin::default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keymap: Arc::new(RuntimeKeymap::defaults()),
            vim_mode: false,
            vim_start: VimModeStart::Normal,
            show_vim_mode: false,
            animations: true,
            title: "Antex".into(),
            mascot: StartupMascotSkin::default(),
            theme: None,
            home: None,
            warnings: Vec::new(),
        }
    }
}

impl Settings {
    pub fn from_config(value: &Value, home: PathBuf) -> Result<Self, String> {
        let raw: RawSettings = serde_json::from_value(value.clone())
            .map_err(|_| "invalid TUI configuration field types")?;
        if raw.startup_panel.title.len() > 128
            || raw.startup_panel.title.chars().any(char::is_control)
            || raw
                .theme
                .as_ref()
                .is_some_and(|theme| theme.len() > 128 || theme.chars().any(char::is_control))
        {
            return Err("TUI configuration exceeds its text budget".into());
        }
        let known = [
            "animations",
            "vim_mode_default",
            "vim_mode_start",
            "show_vim_mode_indicator",
            "startup_panel",
            "keymap",
            "theme",
        ];
        let warnings = value
            .as_object()
            .into_iter()
            .flat_map(|table| table.keys())
            .filter(|key| !known.contains(&key.as_str()))
            .take(64)
            .map(|key| {
                format!(
                    "unsupported TUI configuration key: {}",
                    crate::transcript::safe_text(key)
                )
            })
            .collect();
        let keymap = RuntimeKeymap::from_config(&raw.keymap)
            .map_err(|_| "invalid or conflicting TUI key bindings")?;
        Ok(Self {
            keymap: Arc::new(keymap),
            vim_mode: raw.vim_mode_default,
            vim_start: raw.vim_mode_start,
            show_vim_mode: raw.show_vim_mode_indicator,
            animations: raw.animations,
            title: raw.startup_panel.title,
            mascot: raw.startup_panel.mascot_skin,
            theme: raw.theme,
            home: Some(home),
            warnings,
        })
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
