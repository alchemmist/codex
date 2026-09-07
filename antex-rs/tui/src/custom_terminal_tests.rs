use super::*;
use std::num::NonZeroU16;

use crate::test_backend::VT100Backend;
use insta::assert_snapshot;
use pretty_assertions::assert_eq;
use ratatui::backend::WindowSize;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;

#[path = "custom_terminal/tests/cursor_tests.rs"]
mod cursor;

struct CaptureBackend {
    output: Vec<u8>,
    size: Size,
    cursor: Position,
    size_call_count: std::cell::Cell<usize>,
}

impl CaptureBackend {
    fn new(width: u16, height: u16) -> Self {
        Self {
            output: Vec::new(),
            size: Size { width, height },
            cursor: Position { x: 0, y: 0 },
            size_call_count: std::cell::Cell::new(/*value*/ 0),
        }
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

impl Write for CaptureBackend {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Backend for CaptureBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, _content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        queue!(self, crossterm::cursor::Hide)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        queue!(self, crossterm::cursor::Show)
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.cursor = position.into();
        let Position { x, y } = self.cursor;
        queue!(self, MoveTo(x, y))?;
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clear_region(&mut self, _clear_type: ClearType) -> io::Result<()> {
        Ok(())
    }

    fn append_lines(&mut self, _line_count: u16) -> io::Result<()> {
        Ok(())
    }

    fn scroll_region_up(
        &mut self,
        _region: std::ops::Range<u16>,
        _scroll_by: u16,
    ) -> io::Result<()> {
        Ok(())
    }

    fn scroll_region_down(
        &mut self,
        _region: std::ops::Range<u16>,
        _scroll_by: u16,
    ) -> io::Result<()> {
        Ok(())
    }

    fn size(&self) -> io::Result<Size> {
        self.size_call_count
            .set(self.size_call_count.get().saturating_add(/*rhs*/ 1));
        Ok(self.size)
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: self.size,
            pixels: self.size,
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn invalidate_viewport_repaints_default_style_spaces_over_stale_terminal_cells() {
    let width = 32;
    let height = 2;
    let area = Rect::new(/*x*/ 0, /*y*/ 0, width, height);
    let mut terminal = Terminal::with_options(VT100Backend::new(width, height)).expect("terminal");
    terminal.set_viewport_area(area);

    // History insertion writes directly to the terminal, leaving cells that are absent from
    // the diff buffers. Interior default-style spaces must still overwrite those stale cells.
    write!(
        terminal.backend_mut(),
        "probe-08tcleantwords stale\r\nprobe-09xcleanxwords stale"
    )
    .expect("prefill terminal");
    assert!(
        terminal
            .backend()
            .vt100()
            .screen()
            .contents()
            .contains("probe-08tcleantwords")
    );

    terminal.invalidate_viewport();
    terminal
        .draw(|frame| {
            Paragraph::new(vec![
                Line::from("probe-08 clean words"),
                Line::from("probe-09 clean words"),
            ])
            .render(area, frame.buffer_mut());
        })
        .expect("redraw invalidated viewport");

    assert_snapshot!(terminal.backend().vt100().screen().contents(), @r"
    probe-08 clean words
    probe-09 clean words
    ");
}

#[tokio::test]
async fn leaving_alternate_screen_repaints_restored_inline_viewport() {
    let mut tui = crate::tui::test_support::make_test_tui().expect("test tui");
    let size = tui.terminal.last_known_screen_size;
    let inline = Rect::new(/*x*/ 0, /*y*/ 3, size.width, /*height*/ 6);
    tui.terminal.set_viewport_area(inline);
    tui.enter_alt_screen().expect("enter alternate screen");
    tui.terminal
        .draw(|frame| {
            Paragraph::new(vec![
                Line::from(" Resume a previous session"),
                Line::default(),
                Line::from(" Type to search".dim()),
            ])
            .render(frame.area(), frame.buffer_mut());
        })
        .expect("draw picker");
    tui.leave_alt_screen().expect("leave alternate screen");

    let mut terminal = Terminal::with_options(VT100Backend::new(size.width, size.height))
        .expect("physical terminal");
    write!(terminal.backend_mut(), "Previous conversation").expect("write history");
    terminal.set_viewport_area(inline);
    terminal
        .draw(|frame| {
            Paragraph::new(vec![
                Line::default(),
                Line::default(),
                Line::from(vec!["›".bold(), " ".into(), "/resume".dim()]),
            ])
            .render(frame.area(), frame.buffer_mut());
        })
        .expect("draw submitted command");

    // Apply the real Tui screen-return baseline to the unchanged physical main screen.
    // Matching spaces and letters in the picker must not leave /resume cells behind.
    *terminal.previous_buffer_mut() = tui.terminal.previous_buffer().clone();
    for _ in 0..2 {
        terminal
            .draw(|frame| {
                Paragraph::new(vec![
                    Line::default(),
                    Line::default(),
                    Line::from(vec![
                        "›".bold(),
                        " ".into(),
                        "Ask Codex to do anything".dim(),
                    ]),
                ])
                .render(frame.area(), frame.buffer_mut());
            })
            .expect("restore composer");
    }

    let visible = terminal
        .backend()
        .vt100()
        .screen()
        .contents()
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    assert_snapshot!(visible.trim_end(), @r"
    Previous conversation




    › Ask Codex to do anything
    ");
}

#[test]
fn ordinary_redraws_with_known_size_do_not_query_backend_size() {
    let mut terminal =
        Terminal::with_options(CaptureBackend::new(/*width*/ 80, /*height*/ 24)).expect("terminal");
    let screen_size = terminal.last_known_screen_size;

    for _ in 0..3 {
        terminal.draw_with_size(screen_size, |_| {}).expect("draw");
    }

    terminal.set_viewport_area(Rect::new(
        /*x*/ 0, /*y*/ 23, /*width*/ 80, /*height*/ 1,
    ));
    crate::insert_history::insert_history_lines(&mut terminal, vec![Line::from("history")])
        .expect("insert history");

    assert_eq!(terminal.backend().size_call_count.get(), 1);
}

#[test]
fn resize_draw_applies_event_dimensions_without_querying_backend_size() {
    let mut terminal =
        Terminal::with_options(CaptureBackend::new(/*width*/ 12, /*height*/ 4)).expect("terminal");
    let mut snapshots = Vec::new();

    for width in [12, 8] {
        let size = Size::new(width, /*height*/ 4);
        let area = Rect::new(/*x*/ 0, /*y*/ 0, size.width, size.height);
        terminal.set_viewport_area(area);
        terminal
            .draw_with_size(size, |frame| {
                Paragraph::new("alpha beta")
                    .wrap(Wrap { trim: false })
                    .render(area, frame.buffer_mut());
            })
            .expect("draw resized frame");

        let rendered = (0..size.height)
            .map(|y| {
                (0..size.width)
                    .map(|x| terminal.previous_buffer()[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        snapshots.push(rendered.trim_end().to_string());
    }

    assert_eq!(terminal.backend().size_call_count.get(), 1);
    assert_eq!(
        terminal.last_known_screen_size,
        Size::new(/*width*/ 8, /*height*/ 4)
    );
    assert_snapshot!(snapshots.join("\n\n"), @r"
    alpha beta

    alpha
    beta
    ");
}

#[test]
fn diff_buffers_only_updates_changed_cells_when_row_tails_are_unchanged() {
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 10, /*height*/ 2,
    );
    let mut previous = Buffer::empty(area);
    previous.set_string(0, 0, "-", Style::default());
    previous.set_string(0, 1, "中", Style::default());

    assert_eq!(diff_buffers(&previous, &previous).len(), 0);

    let mut next = previous.clone();
    next.set_string(0, 0, "\\", Style::default());

    let commands = diff_buffers(&previous, &next);
    assert_eq!(commands.len(), 1, "unexpected draw commands: {commands:?}");
    assert!(matches!(
        commands.as_slice(),
        [DrawCommand::Put { x: 0, y: 0, cell }] if cell.symbol() == "\\"
    ));
}

#[test]
fn diff_buffers_does_not_emit_clear_to_end_for_full_width_row() {
    let area = Rect::new(0, 0, 3, 2);
    let previous = Buffer::empty(area);
    let mut next = Buffer::empty(area);

    next.cell_mut((2, 0))
        .expect("cell should exist")
        .set_symbol("X");

    let commands = diff_buffers(&previous, &next);

    let clear_count = commands
        .iter()
        .filter(|command| matches!(command, DrawCommand::ClearToEnd { y, .. } if *y == 0))
        .count();
    assert_eq!(
        0, clear_count,
        "expected diff_buffers not to emit ClearToEnd; commands: {commands:?}",
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Put { x: 2, y: 0, .. })),
        "expected diff_buffers to update the final cell; commands: {commands:?}",
    );
}

#[test]
fn diff_buffers_clear_to_end_starts_after_wide_char() {
    let area = Rect::new(0, 0, 10, 1);
    for (before, after) in [("中文", "中"), ("ｶﾞﾞ", "ｶﾞ")] {
        let mut previous = Buffer::empty(area);
        let mut next = Buffer::empty(area);

        previous.set_string(0, 0, before, Style::default());
        next.set_string(0, 0, after, Style::default());

        let commands = diff_buffers(&previous, &next);
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, DrawCommand::ClearToEnd { x: 2, y: 0, .. })),
            "expected clear-to-end after {before:?} became {after:?}; commands: {commands:?}"
        );
    }

    let mut terminal =
        Terminal::with_options(VT100Backend::new(area.width, area.height)).expect("terminal");
    terminal.set_viewport_area(area);
    for text in ["ｶﾞﾞ", "ｶﾞ"] {
        terminal
            .draw(|frame| Paragraph::new(text).render(area, frame.buffer_mut()))
            .expect("draw");
    }
    assert_snapshot!(terminal.backend().vt100().screen().contents(), @"ｶﾞ");
}

#[test]
fn terminal_draw_coalesces_wrapped_hyperlink_output() {
    let auth_url = format!(
        "https://auth.openai.com/oauth/authorize?response_type=code&state={}",
        "x".repeat(/*n*/ 400)
    );
    let width = 44;
    let height = 20;
    let area = Rect::new(0, 0, width, height);
    let mut terminal =
        Terminal::with_options(CaptureBackend::new(width, height)).expect("terminal");
    terminal.set_viewport_area(area);

    terminal
        .draw(|frame| {
            Paragraph::new(vec![
                Line::from(vec!["  ".into(), auth_url.as_str().cyan().underlined()]),
                "".into(),
                "  Press Esc to cancel".into(),
            ])
            .wrap(Wrap { trim: false })
            .render(area, frame.buffer_mut());
            crate::terminal_hyperlinks::mark_url_hyperlink(frame.buffer_mut(), area, &auth_url);
        })
        .expect("draw");

    let output = terminal.backend().output();
    let open = format!("\x1b]8;;{auth_url}\x07");
    let close = "\x1b]8;;\x07";
    assert_eq!(output.matches(&open).count(), 1);
    assert_eq!(output.matches(close).count(), 1);
    let footer = output.find("Press").expect("footer");
    assert!(output.find(close).expect("hyperlink close") < footer);
}

#[test]
fn terminal_draw_reduces_rgb_and_diff_backgrounds_to_terminal_palette_colors() {
    let draw_cell = |fg: Color, bg: Color| {
        let mut cell = Cell::default();
        cell.set_symbol("x").set_fg(fg).set_bg(bg);
        let mut output = Vec::new();
        draw(
            &mut output,
            [DrawCommand::Put { x: 0, y: 0, cell }].into_iter(),
        )
        .expect("draw cell");
        output
    };

    let actual = draw_cell(
        crate::terminal_palette::rgb_color((220, 40, 40)),
        crate::terminal_palette::rgb_color((32, 32, 32)),
    );
    let expected = draw_cell(Color::Red, Color::Reset);

    assert_eq!(actual, expected);

    let actual_diff = draw_cell(
        Color::Green,
        crate::terminal_palette::rgb_color((33, 58, 43)),
    );
    let expected_diff = draw_cell(Color::Green, Color::Reset);

    assert_eq!(actual_diff, expected_diff);
}

#[test]
fn diff_buffers_emits_always_update_cells() {
    use ratatui::buffer::CellDiffOption;

    for text in ["abc", "a  "] {
        let mut previous = Buffer::with_lines([text]);
        let mut next = Buffer::with_lines([text]);
        previous[(1, 0)].set_diff_option(CellDiffOption::AlwaysUpdate);
        next[(1, 0)].set_diff_option(CellDiffOption::AlwaysUpdate);

        let commands = diff_buffers(&previous, &next);
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, DrawCommand::Put { x: 1, y: 0, .. })),
            "expected the always-update cell in {text:?} to be emitted; commands: {commands:?}"
        );
    }
}

#[test]
fn diff_buffers_clears_styled_trailing_cell_replaced_by_forced_width_cell() {
    use ratatui::buffer::CellDiffOption;

    let area = Rect::new(0, 0, 7, 1);
    let mut previous = Buffer::empty(area);
    let mut next = Buffer::empty(area);
    previous.set_string(
        0,
        0,
        "漢 tail",
        Style::default()
            .bg(Color::Blue)
            .add_modifier(Modifier::UNDERLINED),
    );
    next.set_string(0, 0, "a tail", Style::default());
    next[(0, 0)]
        .set_symbol("\x1b]8;;https://example.com\x07a\x1b]8;;\x07")
        .set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::MIN));

    let commands = diff_buffers(&previous, &next);

    assert!(
        commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Put { x: 1, y: 0, .. })),
        "expected the styled trailing cell to be cleared; commands: {commands:?}"
    );
}

#[test]
fn terminal_draw_moves_cursor_before_showing_it() {
    let cursor_position = Position { x: 1, y: 0 };
    let mut terminal =
        Terminal::with_options(CaptureBackend::new(/*width*/ 2, /*height*/ 1)).expect("terminal");
    terminal.set_viewport_area(Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 2, /*height*/ 1,
    ));

    terminal
        .try_draw(|frame| {
            frame.set_cursor_position(cursor_position);
            io::Result::Ok(())
        })
        .expect("draw");

    let mut expected_move = Vec::new();
    queue!(expected_move, MoveTo(cursor_position.x, cursor_position.y)).expect("queue move");
    let expected_move = String::from_utf8(expected_move).expect("move utf8");
    let mut expected_show = Vec::new();
    queue!(expected_show, crossterm::cursor::Show).expect("queue show");
    let expected_show = String::from_utf8(expected_show).expect("show utf8");
    let actual = terminal.backend().output();
    let move_index = actual.find(&expected_move).expect("cursor move");
    let show_index = actual.find(&expected_show).expect("cursor show");

    assert!(
        move_index < show_index,
        "expected cursor move before show, got {actual:?}"
    );
    assert_snapshot!(
        actual[move_index..].escape_debug().to_string(),
        @r"\u{1b}[1;2H\u{1b}[?25h"
    );
}

#[test]
fn reset_cursor_style_emits_default_user_shape() {
    let mut output = Vec::new();
    let mut terminal =
        Terminal::with_options(CaptureBackend::new(/*width*/ 2, /*height*/ 1)).expect("terminal");

    terminal.reset_cursor_style().expect("reset cursor style");
    ratatui::backend::Backend::flush(terminal.backend_mut()).expect("flush backend");

    queue!(output, SetCursorStyle::DefaultUserShape).expect("queue style");
    let expected = String::from_utf8(output).expect("utf8");
    let actual = terminal.backend().output();
    assert!(
        actual.contains(&expected),
        "expected terminal output to contain cursor style reset {expected:?}, got {actual:?}"
    );
}
