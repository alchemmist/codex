use crate::Session;
use crate::composer::Composer;
use crate::transcript::safe_text;
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
    prompt: &mut Option<crate::prompt::Prompt>,
    startup: &Option<crate::startup::Startup>,
) -> io::Result<()> {
    let size = tui.terminal.size()?;
    if size.width == 0 || size.height == 0 {
        return Ok(());
    }
    let view = session.view();
    let header = startup
        .as_ref()
        .map(|panel| panel.lines(size.width))
        .unwrap_or_default();
    let header_height = header.len().min(usize::from(size.height.saturating_sub(3))) as u16;
    let live_lines = crate::wrapping::word_wrap_lines(
        live.lines().map(Line::from),
        usize::from(size.width.max(1)),
    );
    let live_height =
        (live_lines.len().min(8) as u16).min(size.height.saturating_sub(header_height + 3));
    let height =
        (header_height + composer.height(size.width).min(8) + live_height + 2).min(size.height);
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
        Paragraph::new(live_lines[first..].to_vec()).render(live_area, frame.buffer_mut());
        let editor_area = Rect {
            y: area.y + live_area.height,
            height: area.height.saturating_sub(live_area.height + 2),
            ..area
        };
        let cursor = match prompt {
            Some(pane) => pane.render(editor_area, frame.buffer_mut()),
            None => composer.render(editor_area, frame.buffer_mut()),
        };
        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }
        let mut footer = safe_text(&format!(
            "{} · {} · {}",
            view.model,
            view.directory,
            view.permissions,
        ));
        if let Some(mode) = composer.mode_label() { footer.push_str(&format!(" · {mode}")); }
        if area.height >= 2 {
            Line::from(footer.dim()).render(
                Rect {
                    y: area.bottom() - 2,
                    height: 1,
                    ..area
                },
                frame.buffer_mut(),
            );
            Line::from(status.to_owned().dim()).render(
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
