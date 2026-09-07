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

pub(crate) fn terminal_foreground(color: Color) -> Color {
    if crate::mascot_palette::is_preserved_mascot_color(color) {
        return color;
    }
    match color {
        Color::Reset | Color::Black | Color::Gray | Color::DarkGray | Color::White => Color::Reset,
        Color::Rgb(red, green, blue) => accent(red, green, blue),
        Color::Indexed(index) => terminal_foreground(indexed_rgb(index)),
        Color::Red
        | Color::Green
        | Color::Yellow
        | Color::Blue
        | Color::Magenta
        | Color::Cyan
        | Color::LightRed
        | Color::LightGreen
        | Color::LightYellow
        | Color::LightBlue
        | Color::LightMagenta
        | Color::LightCyan => color,
    }
}

pub(crate) fn terminal_background(color: Color) -> Color {
    if crate::mascot_palette::is_preserved_mascot_color(color) {
        return color;
    }
    match color {
        Color::Reset | Color::Black | Color::Gray | Color::White | Color::Rgb(..) => Color::Reset,
        Color::Indexed(index) if index < 16 => terminal_background(indexed_rgb(index)),
        Color::Indexed(_) => Color::Reset,
        Color::DarkGray
        | Color::Red
        | Color::Green
        | Color::Yellow
        | Color::Blue
        | Color::Magenta
        | Color::Cyan
        | Color::LightRed
        | Color::LightGreen
        | Color::LightYellow
        | Color::LightBlue
        | Color::LightMagenta
        | Color::LightCyan => color,
    }
}

fn accent(red: u8, green: u8, blue: u8) -> Color {
    let low = red.min(green).min(blue);
    let chroma = red.max(green).max(blue).saturating_sub(low);
    if chroma < 32 {
        return Color::Reset;
    }
    let middle = low.saturating_add(chroma / 2);
    match (red > middle, green > middle, blue > middle) {
        (true, false, false) => Color::Red,
        (false, true, false) => Color::Green,
        (false, false, true) => Color::Blue,
        (true, true, false) => Color::Yellow,
        (true, false, true) => Color::Magenta,
        (false, true, true) => Color::Cyan,
        (true, true, true) | (false, false, false) => Color::Reset,
    }
}

fn indexed_rgb(index: u8) -> Color {
    const SYSTEM: [Color; 16] = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ];
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    if index < 16 {
        return SYSTEM[index as usize];
    }
    if index >= 232 {
        return Color::Gray;
    }
    let index = usize::from(index - 16);
    Color::Rgb(CUBE[index / 36], CUBE[index / 6 % 6], CUBE[index % 6])
}
