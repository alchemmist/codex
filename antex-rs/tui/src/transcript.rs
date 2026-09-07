use crate::terminal_hyperlinks::HyperlinkLine;
use crate::terminal_hyperlinks::plain_hyperlink_lines;

pub(crate) fn tool_preview(
    call: &antex_core::ToolCall,
    width: usize,
    cwd: &std::path::Path,
) -> Vec<Line<'static>> {
    let Some(path) = call.arguments["path"].as_str() else {
        return Vec::new();
    };
    let (title, change) = match call.name.as_str() {
        "edit" => {
            let (Some(old), Some(new)) = (
                call.arguments["old_text"].as_str(),
                call.arguments["new_text"].as_str(),
            ) else {
                return Vec::new();
            };
            let patch = diffy::create_patch(&safe_text(old), &safe_text(new)).to_string();
            (
                "Proposed edit of the matching text",
                crate::diff_model::FileChange::Update {
                    unified_diff: patch,
                    move_path: None,
                },
            )
        }
        "write" => {
            let Some(content) = call.arguments["content"].as_str() else {
                return Vec::new();
            };
            (
                "Proposed full file contents",
                crate::diff_model::FileChange::Add {
                    content: safe_text(content),
                },
            )
        }
        _ => return Vec::new(),
    };
    let changes =
        std::collections::HashMap::from([(std::path::PathBuf::from(safe_text(path)), change)]);
    let mut lines = vec![Line::from(title.dim())];
    lines.extend(crate::diff_render::create_diff_summary_for(
        &changes,
        cwd,
        width,
        crate::diff_render::DiffStage::Proposed,
    ));
    lines
}
use antex_core::Content;
use antex_core::Message;
use ratatui::style::Stylize;
use ratatui::text::Line;

pub(crate) fn write_message<
    B: ratatui::backend::Backend<Error = std::io::Error> + std::io::Write,
>(
    terminal: &mut crate::custom_terminal::Terminal<B>,
    message: &Message,
    cwd: &std::path::Path,
) -> std::io::Result<()> {
    let size = terminal.last_known_screen_size;
    let lines = message_lines(message, usize::from(size.width.max(1)), cwd);
    crate::insert_history::insert_history_hyperlink_lines(terminal, &lines, size)
}

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

fn message_lines(message: &Message, width: usize, cwd: &std::path::Path) -> Vec<HyperlinkLine> {
    let (label, text) = match message {
        Message::Context(_) => return Vec::new(),
        Message::User(input) => ("›", content_text(&input.content)),
        Message::Assistant { content, .. } => ("Antex", content_text(content)),
        Message::Tool(output) => ("tool", output.text().to_owned()),
    };
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = plain_hyperlink_lines(vec![Line::from(label.to_owned().bold())]);
    let text = safe_text(&text);
    if matches!(message, Message::Assistant { .. }) {
        lines.extend(crate::markdown::render_markdown_agent_with_links_and_cwd(
            &text,
            Some(width),
            Some(cwd),
        ));
    } else {
        lines.extend(plain_hyperlink_lines(
            text.lines()
                .map(|line| Line::from(line.to_owned()))
                .collect(),
        ));
    }
    if lines.len() > 2048 {
        lines.truncate(2048);
        lines.extend(plain_hyperlink_lines(vec![Line::from(
            "[display truncated; full content remains in the session]",
        )]));
    }
    lines.extend(plain_hyperlink_lines(vec![Line::default()]));
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

pub(crate) fn assistant_text(message: &Message) -> Option<String> {
    if let Message::Assistant { content, .. } = message {
        let text = content_text(content);
        return (!text.is_empty()).then_some(text);
    }
    None
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
