use ratatui::style::Color;

#[derive(Clone, Copy)]
pub(crate) enum StdoutColorLevel {
    TrueColor,
    Ansi256,
    Ansi16,
    Unknown,
}

pub(crate) fn effective_stdout_color_level() -> StdoutColorLevel {
    if std::env::var_os("NO_COLOR").is_some() {
        return StdoutColorLevel::Unknown;
    }
    if std::env::var("COLORTERM").is_ok_and(|value| matches!(value.as_str(), "truecolor" | "24bit"))
    {
        return StdoutColorLevel::TrueColor;
    }
    match std::env::var("TERM") {
        Ok(term) if term.contains("256color") => StdoutColorLevel::Ansi256,
        Ok(term) if term != "dumb" => StdoutColorLevel::Ansi16,
        _ => StdoutColorLevel::Unknown,
    }
}

pub(crate) fn indexed_color(index: u8) -> Color {
    Color::Indexed(index)
}
pub(crate) fn rgb_color((red, green, blue): (u8, u8, u8)) -> Color {
    Color::Rgb(red, green, blue)
}
