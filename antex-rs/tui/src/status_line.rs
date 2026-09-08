use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

pub(crate) fn render(
    fields: &[String],
    use_theme_colors: bool,
    view: &crate::SessionView,
) -> Line<'static> {
    let mut spans = Vec::new();
    for field in fields {
        let (text, scopes, fallback): (_, &[&str], _) = match field.as_str() {
            "model" | "model-name" => (
                crate::transcript::safe_text(&view.model),
                &["entity.name.type", "support.type", "variable"],
                Style::default().cyan(),
            ),
            "current-dir" => (
                crate::transcript::display_directory(&view.directory),
                &["string", "markup.underline.link"],
                Style::default().green(),
            ),
            _ => continue,
        };
        spans.push(if spans.is_empty() {
            "  ".into()
        } else {
            " · ".dim()
        });
        let style = if use_theme_colors {
            soften(
                crate::render::highlight::foreground_style_for_scopes(scopes).unwrap_or(fallback),
            )
        } else {
            Style::default().dim()
        };
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

fn soften(mut style: Style) -> Style {
    style.fg = style.fg.map(|color| match color {
        Color::Rgb(r, g, b) => {
            let luma = (77 * u16::from(r) + 150 * u16::from(g) + 29 * u16::from(b)) / 256;
            let channel = |channel| ((u16::from(channel) * 85 + luma * 15 + 50) / 100) as u8;
            crate::terminal_palette::rgb_color((channel(r), channel(g), channel(b)))
        }
        Color::LightRed => Color::Red,
        Color::LightGreen => Color::Green,
        Color::LightYellow => Color::Yellow,
        Color::LightBlue => Color::Blue,
        Color::LightMagenta => Color::Magenta,
        Color::LightCyan => Color::Cyan,
        Color::White => Color::Gray,
        Color::Reset
        | Color::Black
        | Color::Red
        | Color::Green
        | Color::Yellow
        | Color::Blue
        | Color::Magenta
        | Color::Cyan
        | Color::Gray
        | Color::DarkGray
        | Color::Indexed(_) => color,
    });
    style
}
