use std::io;

use crossterm::execute;
use crossterm::terminal;

pub(crate) struct TerminalGuard {
    restore_raw: bool,
}

impl TerminalGuard {
    pub(crate) fn enter() -> io::Result<Self> {
        let restore_raw = !terminal::is_raw_mode_enabled()?;
        let guard = Self { restore_raw };
        terminal::enable_raw_mode()?;
        execute!(
            io::stdout(),
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableFocusChange
        )?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableFocusChange,
            crossterm::cursor::SetCursorStyle::DefaultUserShape,
            crossterm::cursor::Show,
            crossterm::style::ResetColor
        );
        if self.restore_raw {
            let _ = terminal::disable_raw_mode();
        }
    }
}
