use crate::terminal_hyperlinks::HyperlinkLine;

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

pub(crate) fn display_directory(path: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(relative) = path.strip_prefix(std::path::Path::new(&home))
    {
        return if relative.as_os_str().is_empty() {
            "~".into()
        } else {
            safe_text(&format!("~/{}", relative.display()))
        };
    }
    safe_text(&path.display().to_string())
}

pub(crate) fn message_lines(
    message: &Message,
    width: usize,
    cwd: &std::path::Path,
) -> Vec<HyperlinkLine> {
    let text = match message {
        Message::Context(_) => return Vec::new(),
        Message::User(input) => content_text(&input.content),
        Message::Assistant { content, .. } => content_text(content),
        Message::Tool(output) => output.text().to_owned(),
    };
    if text.is_empty() {
        return Vec::new();
    }
    let text = safe_text(&text);
    let mut lines = match message {
        Message::User(_) => {
            let mut lines = vec![HyperlinkLine::default()];
            for text in text.lines() {
                let logical = crate::terminal_hyperlinks::annotate_web_urls_in_line(Line::from(
                    text.to_owned(),
                ));
                let wrapped = crate::wrapping::word_wrap_line(
                    &logical.line,
                    crate::wrapping::RtOptions::new(width.saturating_sub(4).max(1))
                        .break_words(true),
                )
                .iter()
                .map(crate::render::line_utils::line_to_static)
                .collect();
                lines.extend(
                    crate::terminal_hyperlinks::remap_wrapped_line(&logical, wrapped)
                        .into_iter()
                        .map(|line| {
                            prefix(
                                line,
                                ratatui::text::Span::styled(" ┃ ", crate::style::accent_style()),
                            )
                        }),
                );
            }
            lines
        }
        Message::Assistant { .. } => assistant_lines(&text, width, cwd),
        Message::Tool(_) => text
            .lines()
            .map(|line| HyperlinkLine::new(Line::from(format!("  └ {line}")).dim()))
            .collect(),
        Message::Context(_) => unreachable!(),
    };
    if lines.len() > 2048 {
        lines.truncate(2048);
        lines.push(HyperlinkLine::new(Line::from(
            "[display truncated; full content remains in the session]",
        )));
    }
    lines.push(HyperlinkLine::default());
    lines
}

pub(crate) fn assistant_lines(
    text: &str,
    width: usize,
    cwd: &std::path::Path,
) -> Vec<HyperlinkLine> {
    crate::markdown::render_markdown_agent_with_links_and_cwd(
        text,
        Some(width.saturating_sub(2).max(1)),
        Some(cwd),
    )
    .into_iter()
    .enumerate()
    .map(|(index, line)| {
        prefix(
            line,
            if index == 0 {
                "• ".dim()
            } else {
                "  ".into()
            },
        )
    })
    .collect()
}

fn prefix(mut line: HyperlinkLine, prefix: ratatui::text::Span<'static>) -> HyperlinkLine {
    let width = crate::width::display_width(&prefix.content);
    for link in &mut line.hyperlinks {
        link.columns = link.columns.start + width..link.columns.end + width;
    }
    line.line.spans.insert(0, prefix);
    line
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
