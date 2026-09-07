use std::io;

use crossterm::execute;
use crossterm::terminal;

pub(crate) struct TerminalGuard {
    restore_raw: bool,
    enhanced: bool,
}

impl TerminalGuard {
    pub(crate) fn enter() -> io::Result<Self> {
        let restore_raw = !terminal::is_raw_mode_enabled()?;
        let guard = Self {
            restore_raw,
            enhanced: false,
        };
        terminal::enable_raw_mode()?;
        execute!(
            io::stdout(),
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableFocusChange
        )?;
        Ok(guard)
    }

    pub(crate) fn enable_enhanced_keys(&mut self) -> io::Result<()> {
        use crossterm::event::KeyboardEnhancementFlags;
        execute!(
            io::stdout(),
            crossterm::event::PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        )?;
        self.enhanced = true;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.enhanced {
            let _ = execute!(io::stdout(), crossterm::event::PopKeyboardEnhancementFlags);
        }
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
