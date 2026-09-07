use antex_core::Content;
use antex_core::Message;
use ratatui::style::Stylize;
use ratatui::text::Line;

pub(crate) fn safe_text(text: &str) -> String {
    let mut visible = String::new();
    for ch in text.chars() {
        if visible.len() >= 64 * 1024 {
            visible.push_str("\n[display truncated; full content remains in the session]");
            break;
        }
        if ch.is_control() && !matches!(ch, '\n' | '\t') {
            visible.extend(ch.escape_default());
        } else {
            visible.push(ch);
        }
    }
    visible
}

pub(crate) fn message_lines(message: &Message) -> Vec<Line<'static>> {
    let (label, text) = match message {
        Message::Context(_) => return Vec::new(),
        Message::User(input) => ("›", content_text(&input.content)),
        Message::Assistant { content, .. } => ("Antex", content_text(content)),
        Message::Tool(output) => ("tool", output.text().to_owned()),
    };
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Line::from(label.to_owned().bold())];
    lines.extend(
        safe_text(&text)
            .lines()
            .map(|line| Line::from(line.to_owned())),
    );
    lines.push(Line::default());
    lines
}

fn content_text(content: &[Content]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            Content::Text(text) => Some(text.as_str()),
            Content::Image { .. } => Some("[Image]"),
            Content::Reasoning(_) | Content::Continuation { .. } => None,
        })
        .collect()
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
