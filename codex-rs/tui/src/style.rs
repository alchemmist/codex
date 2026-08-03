use crate::color::blend;
use crate::color::is_light;
use ratatui::style::Color;
use ratatui::style::Style;

pub fn user_message_style() -> Style {
    Style::default()
}

pub fn proposed_plan_style() -> Style {
    Style::default()
}

/// Returns a low-contrast rule style for separators within markdown tables.
pub(crate) fn table_separator_style() -> Style {
    Style::default().dim()
}

/// Returns the shared accent style for active or selected TUI controls.
pub(crate) fn accent_style() -> Style {
    Style::default().fg(Color::Cyan).bold()
}

pub(crate) fn user_message_bg_rgb(terminal_bg: (u8, u8, u8)) -> (u8, u8, u8) {
    let (top, alpha) = if is_light(terminal_bg) {
        ((0, 0, 0), 0.04)
    } else {
        ((255, 255, 255), 0.12)
    };
    blend(top, terminal_bg, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_surface_styles_snapshot() {
        insta::assert_debug_snapshot!(
            "terminal_surface_styles",
            [
                ("user message", user_message_style()),
                ("proposed plan", proposed_plan_style()),
                ("table separator", table_separator_style()),
                ("accent", accent_style()),
            ]
        );
    }
}
