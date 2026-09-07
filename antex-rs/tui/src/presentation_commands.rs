use crate::CommandEffect;
use crate::PickerItem;
use crate::PickerSpec;
use crate::Session;
use crate::Settings;

pub(crate) fn handle(
    command: &str,
    settings: &Settings,
    session: &mut impl Session,
) -> Option<Result<CommandEffect, String>> {
    let (name, argument) = command
        .trim()
        .split_once(char::is_whitespace)
        .unwrap_or((command.trim(), ""));
    if name != "/theme" {
        return None;
    }
    let argument = argument.trim();
    if argument.is_empty() {
        let mut items = vec![PickerItem {
            label: "Automatic".into(),
            description: "Follow the terminal palette".into(),
            command: "/theme auto".into(),
        }];
        items.extend(
            crate::render::highlight::list_available_themes(settings.home.as_deref())
                .into_iter()
                .filter(|theme| theme.name.len() <= 128)
                .take(255)
                .map(|theme| PickerItem {
                    label: theme.name.clone(),
                    description: if theme.is_custom {
                        "Custom theme"
                    } else {
                        "Bundled theme"
                    }
                    .into(),
                    command: format!("/theme {}", theme.name),
                }),
        );
        return Some(Ok(CommandEffect::Picker(PickerSpec {
            title: "Syntax theme".into(),
            items,
        })));
    }
    if argument.len() > 128
        || argument.contains(['/', '\\'])
        || argument.chars().any(char::is_control)
    {
        return Some(Err("Invalid theme name.".into()));
    }
    let name = if argument == "auto" {
        crate::render::highlight::adaptive_default_theme_name()
    } else {
        argument
    };
    let Some(theme) =
        crate::render::highlight::resolve_theme_by_name(name, settings.home.as_deref())
    else {
        return Some(Err("Unknown theme; use /theme to choose one.".into()));
    };
    if let Err(error) =
        session.save_ui_state("syntaxTheme", &serde_json::Value::String(argument.into()))
    {
        return Some(Err(error));
    }
    crate::render::highlight::set_syntax_theme(theme);
    Some(Ok(CommandEffect::Notice(format!("Theme: {argument}"))))
}

pub(crate) fn restore(settings: &Settings, session: &mut impl Session) -> Result<(), String> {
    let saved = match session.load_ui_state("syntaxTheme")? {
        Some(serde_json::Value::String(name)) => Some(name),
        None => None,
        Some(_) => return Err("Invalid saved theme state.".into()),
    };
    let name = saved.or_else(|| settings.theme.clone());
    if name.is_none() && crate::render::highlight::syntax_theme_revision() == 0 {
        return Ok(());
    }
    let name = name.as_deref().unwrap_or("auto");
    if name.len() > 128 || name.contains(['/', '\\']) || name.chars().any(char::is_control) {
        return Err("Invalid saved theme name.".into());
    }
    let name = if name == "auto" {
        crate::render::highlight::adaptive_default_theme_name()
    } else {
        name
    };
    if let Some(theme) =
        crate::render::highlight::resolve_theme_by_name(name, settings.home.as_deref())
    {
        crate::render::highlight::set_syntax_theme(theme);
    }
    Ok(())
}
