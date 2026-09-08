use crate::Session;
use crate::composer::Composer;
use crate::tui::Tui;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::io;

pub(super) fn draw<B: ratatui::backend::Backend<Error = io::Error> + io::Write>(
    tui: &mut Tui<B>,
    composer: &mut Composer,
    session: &impl Session,
    status: &str,
    live: &str,
    prompt: &mut Option<crate::overlay::Overlay>,
    startup: &Option<crate::startup::Startup>,
) -> io::Result<()> {
    let size = tui.terminal.size()?;
    if size.width == 0 || size.height == 0 {
        return Ok(());
    }
    let view = session.view();
    let input_height = prompt
        .as_ref()
        .map(crate::overlay::Overlay::height)
        .unwrap_or_else(|| composer.height(size.width).min(8));
    let working = prompt
        .is_none()
        .then(|| composer.working_line(size.width))
        .flatten();
    let working_height = u16::from(working.is_some());
    let reserved = 1 + working_height;
    let header_room = size.height.saturating_sub(input_height + reserved);
    let header = if prompt.is_some() || header_room < 2 {
        Vec::new()
    } else {
        startup
            .as_ref()
            .map(|panel| {
                panel.lines(
                    if header_room < 6 {
                        size.width.min(35)
                    } else {
                        size.width
                    },
                    &view,
                )
            })
            .unwrap_or_default()
    };
    let header = if header.len() > usize::from(header_room) {
        Vec::new()
    } else {
        header
    };
    let header_height = header.len() as u16;
    let live_lines = if live.is_empty() {
        Vec::new()
    } else {
        crate::transcript::assistant_lines(live, usize::from(size.width), &view.directory)
    };
    let live_height = (live_lines.len().min(8) as u16).min(
        size.height
            .saturating_sub(header_height + input_height + reserved),
    );
    let height = (header_height + input_height + live_height + reserved).min(size.height);
    let previous = tui.terminal.viewport_area;
    let y = previous.y.min(size.height.saturating_sub(height));
    if previous.y + height > size.height {
        crossterm::execute!(
            tui.terminal.backend_mut(),
            crossterm::cursor::MoveTo(0, size.height.saturating_sub(1)),
            crossterm::style::Print("\r\n".repeat(usize::from(previous.y + height - size.height)))
        )?;
    }
    let viewport = Rect::new(/*x*/ 0, y, size.width, height);
    if viewport != previous {
        tui.terminal.clear_after_position(viewport.as_position())?;
        tui.terminal.set_viewport_area(viewport);
    }
    tui.terminal.draw(|frame| {
        let area = frame.area();
        Paragraph::new(header).render(
            Rect {
                height: header_height,
                ..area
            },
            frame.buffer_mut(),
        );
        let area = Rect {
            y: area.y + header_height,
            height: area.height.saturating_sub(header_height),
            ..area
        };
        let live_area = Rect {
            height: live_height.min(area.height),
            ..area
        };
        let first = live_lines
            .len()
            .saturating_sub(usize::from(live_area.height));
        crate::terminal_hyperlinks::HyperlinkParagraph::new(
            &live_lines[first..],
            ratatui::style::Style::default(),
        )
        .render(live_area, frame.buffer_mut());
        if let Some(working) = working {
            working.render(
                Rect::new(area.x, area.y + live_area.height, area.width, 1),
                frame.buffer_mut(),
            );
        }
        let editor_area = Rect {
            y: area.y + live_area.height + working_height,
            height: area.height.saturating_sub(live_area.height + reserved),
            ..area
        };
        let cursor = match prompt {
            Some(pane) => pane.render(editor_area, frame.buffer_mut()),
            None => composer.render(editor_area, frame.buffer_mut()),
        };
        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }
        let mut footer = if working_height == 0 && !status.is_empty() {
            Line::from(format!("  {status}").dim())
        } else {
            composer.footer(&view)
        };
        if let Some(mode) = composer.mode_label() {
            footer.spans.push(format!(" · {mode}").dim());
        }
        if composer.has_stash() {
            footer.spans.insert(0, "  stashed · ".dim());
        }
        if area.height >= 1 {
            crate::line_truncation::truncate_line_with_ellipsis_if_overflow(
                footer,
                usize::from(area.width),
            )
            .render(
                Rect {
                    y: area.bottom() - 1,
                    height: 1,
                    ..area
                },
                frame.buffer_mut(),
            );
        }
    })
}
