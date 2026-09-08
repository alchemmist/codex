use std::collections::VecDeque;
use std::io;

use antex_core::Message;
use ratatui::layout::Rect;

use crate::terminal_hyperlinks::HyperlinkLine;

const MAX_ROWS: usize = 1000;

pub(crate) fn replay<B: ratatui::backend::Backend<Error = io::Error> + io::Write>(
    tui: &mut crate::tui::Tui<B>,
    history: &[Message],
    startup: Option<&crate::startup::Startup>,
    view: &crate::SessionView,
    input_height: u16,
) -> io::Result<()> {
    let size = tui.terminal.size()?;
    if history.is_empty() || size.width == 0 || size.height < 2 {
        return Ok(());
    }
    let mut chunks = VecDeque::new();
    let mut count = 0usize;
    for message in history.iter().rev() {
        let lines =
            crate::transcript::message_lines(message, usize::from(size.width), &view.directory);
        count += lines.len();
        chunks.push_front(lines);
        if count > MAX_ROWS {
            break;
        }
    }
    let mut lines = Vec::new();
    if count <= MAX_ROWS
        && let Some(startup) = startup
    {
        lines.extend(
            startup
                .final_lines(size.width, view)
                .into_iter()
                .map(HyperlinkLine::new),
        );
    }
    lines.extend(chunks.into_iter().flatten());
    if lines.len() > MAX_ROWS {
        lines.drain(..lines.len() - MAX_ROWS + 1);
        lines.insert(
            0,
            HyperlinkLine::new(
                crate::line_truncation::truncate_line_with_ellipsis_if_overflow(
                    ratatui::text::Line::from("… Older messages are available in /transcript"),
                    usize::from(size.width),
                ),
            ),
        );
    }
    tui.terminal.clear_scrollback_and_visible_screen_ansi()?;
    tui.terminal.resize(size)?;
    tui.terminal.set_viewport_area(Rect::new(
        0,
        0,
        size.width,
        input_height.min(size.height - 1).max(1),
    ));
    crate::insert_history::insert_history_hyperlink_lines(&mut tui.terminal, &lines, size)
}

#[cfg(test)]
#[path = "history_reflow_tests.rs"]
mod tests;
