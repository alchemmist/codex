use crate::color::blend;
use crate::color::is_light;
use crate::render::highlight::current_syntax_theme_base_colors;
use crate::terminal_palette::StdoutColorLevel;
use crate::terminal_palette::best_color;
use crate::terminal_palette::best_color_for_level;
use crate::terminal_palette::default_bg;
use crate::terminal_palette::default_fg;
use crate::terminal_palette::effective_stdout_color_level;
use crate::terminal_palette::rgb_color;
use crate::terminal_palette::stdout_color_level;
use ratatui::style::Color;
use ratatui::style::Style;

const LIGHT_BG_ACCENT_RGB: (u8, u8, u8) = (0, 95, 135);
// Decorative table rules should remain visible without competing with cell content.
const TABLE_SEPARATOR_FG_ALPHA: f32 = 0.20;

pub fn user_message_style() -> Style {
    let theme = current_syntax_theme_base_colors();
    let background = theme.background.or_else(default_bg);
    match theme.foreground {
        Some(foreground) => user_message_style_for_theme(background, Some(foreground)),
        None => user_message_style_for(background),
    }
}

pub fn proposed_plan_style() -> Style {
    proposed_plan_style_for(default_bg())
}

/// Returns a low-contrast rule style for separators within markdown tables.
pub(crate) fn table_separator_style() -> Style {
    table_separator_style_for(default_fg(), default_bg(), stdout_color_level())
}

/// Returns the shared accent style for active or selected TUI controls.
pub(crate) fn accent_style() -> Style {
    accent_style_for(default_bg())
}

/// Returns the style for a user-authored message using the provided terminal background.
pub fn user_message_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    user_message_style_for_theme(terminal_bg, /*theme_fg*/ None)
}

fn user_message_style_for_theme(
    background: Option<(u8, u8, u8)>,
    theme_fg: Option<(u8, u8, u8)>,
) -> Style {
    user_message_style_for_theme_and_level(background, theme_fg, effective_stdout_color_level())
}

fn user_message_style_for_theme_and_level(
    background: Option<(u8, u8, u8)>,
    theme_fg: Option<(u8, u8, u8)>,
    color_level: StdoutColorLevel,
) -> Style {
    let mut style = Style::default();
    if let Some(bg) = background {
        style = style.bg(best_color_for_level(user_message_bg_rgb(bg), color_level));
    }
    if let Some(fg) = theme_fg {
        style = style.fg(best_color_for_level(fg, color_level));
    }
    style
}

pub fn proposed_plan_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    match terminal_bg {
        Some(bg) => Style::default().bg(proposed_plan_bg(bg)),
        None => Style::default(),
    }
}

/// Returns the shared accent style for the provided terminal background.
pub(crate) fn accent_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    if terminal_bg.is_some_and(is_light) {
        Style::default().fg(best_color(LIGHT_BG_ACCENT_RGB)).bold()
    } else {
        Style::default().fg(Color::Cyan).bold()
    }
}

fn table_separator_style_for(
    terminal_fg: Option<(u8, u8, u8)>,
    terminal_bg: Option<(u8, u8, u8)>,
    color_level: StdoutColorLevel,
) -> Style {
    let (Some(fg), Some(bg)) = (terminal_fg, terminal_bg) else {
        return Style::default().dim();
    };
    let separator_rgb = blend(fg, bg, TABLE_SEPARATOR_FG_ALPHA);
    match color_level {
        StdoutColorLevel::TrueColor => Style::default().fg(rgb_color(separator_rgb)),
        StdoutColorLevel::Ansi256 => Style::default().fg(best_color(separator_rgb)),
        StdoutColorLevel::Ansi16 | StdoutColorLevel::Unknown => Style::default().dim(),
    }
}

#[allow(clippy::disallowed_methods)]
pub fn user_message_bg(terminal_bg: (u8, u8, u8)) -> Color {
    best_color(user_message_bg_rgb(terminal_bg))
}

pub(crate) fn user_message_bg_rgb(terminal_bg: (u8, u8, u8)) -> (u8, u8, u8) {
    let (top, alpha) = if is_light(terminal_bg) {
        ((0, 0, 0), 0.04)
    } else {
        ((255, 255, 255), 0.12)
    };
    blend(top, terminal_bg, alpha)
}

#[allow(clippy::disallowed_methods)]
pub fn proposed_plan_bg(terminal_bg: (u8, u8, u8)) -> Color {
    user_message_bg(terminal_bg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ratatui::style::Modifier;

    #[test]
    fn accent_style_uses_darker_cyan_on_light_backgrounds() {
        let style = accent_style_for(Some((255, 255, 255)));

        assert_eq!(style.fg, Some(best_color(LIGHT_BG_ACCENT_RGB)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn accent_style_uses_cyan_on_dark_or_unknown_backgrounds() {
        let expected = Style::default().fg(Color::Cyan).bold();

        assert_eq!(accent_style_for(Some((0, 0, 0))), expected);
        assert_eq!(accent_style_for(/*terminal_bg*/ None), expected);
    }

    #[test]
    fn table_separator_blends_toward_dark_background() {
        let style = table_separator_style_for(
            Some((255, 255, 255)),
            Some((0, 0, 0)),
            StdoutColorLevel::TrueColor,
        );

        assert_eq!(style.fg, Some(rgb_color((51, 51, 51))));
    }

    #[test]
    fn table_separator_blends_toward_light_background() {
        let style = table_separator_style_for(
            Some((0, 0, 0)),
            Some((255, 255, 255)),
            StdoutColorLevel::TrueColor,
        );

        assert_eq!(style.fg, Some(rgb_color((204, 204, 204))));
    }

    #[test]
    fn table_separator_dims_when_palette_aware_color_is_unavailable() {
        let expected = Style::default().dim();

        assert_eq!(
            table_separator_style_for(
                Some((255, 255, 255)),
                Some((0, 0, 0)),
                StdoutColorLevel::Ansi16,
            ),
            expected
        );
        assert_eq!(
            table_separator_style_for(
                /*terminal_fg*/ None,
                Some((0, 0, 0)),
                StdoutColorLevel::TrueColor,
            ),
            expected
        );
    }

    #[test]
    fn user_message_style_tracks_theme_base_colors_snapshot() {
        let ansi =
            crate::render::highlight::resolve_theme_by_name("ansi", /*codex_home*/ None)
                .expect("expected built-in ANSI theme to load");
        let github =
            crate::render::highlight::resolve_theme_by_name("github", /*codex_home*/ None)
                .expect("expected built-in GitHub theme to load");
        let ansi = crate::render::highlight::syntax_theme_base_colors(&ansi);
        let github = crate::render::highlight::syntax_theme_base_colors(&github);

        insta::assert_debug_snapshot!(
            "user_message_style_theme_base_colors",
            [
                (
                    "ansi dark",
                    user_message_style_for_theme_and_level(
                        ansi.background,
                        ansi.foreground,
                        StdoutColorLevel::TrueColor,
                    ),
                ),
                (
                    "github light",
                    user_message_style_for_theme_and_level(
                        github.background,
                        github.foreground,
                        StdoutColorLevel::TrueColor,
                    ),
                ),
            ]
        );
    }
}
