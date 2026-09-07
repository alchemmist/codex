use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use tokio::sync::Notify;

#[derive(Clone, Debug)]
pub(crate) struct FrameRequester(Arc<FrameClock>);

#[derive(Debug)]
struct FrameClock {
    next: Mutex<Option<Instant>>,
    changed: Notify,
}

impl FrameRequester {
    pub(crate) fn new() -> Self {
        Self(Arc::new(FrameClock {
            next: Mutex::new(None),
            changed: Notify::new(),
        }))
    }

    pub(crate) fn schedule_frame_in(&self, delay: Duration) {
        let deadline = Instant::now() + delay;
        let mut next = self
            .0
            .next
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if next.is_none_or(|current| deadline < current) {
            *next = Some(deadline);
            self.0.changed.notify_one();
        }
    }

    pub(crate) async fn next_frame(&self) {
        loop {
            let changed = self.0.changed.notified();
            let next = *self
                .0
                .next
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(deadline) = next {
                tokio::select! {
                    _=tokio::time::sleep_until(deadline.into())=>{
                        let mut next=self.0.next.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if next.is_some_and(|deadline|deadline<=Instant::now()) {*next=None;return;}
                    }
                    _=changed=>{},
                }
            } else {
                changed.await;
            }
        }
    }
}

pub(crate) struct Tui<B: ratatui::backend::Backend<Error = std::io::Error> + std::io::Write> {
    pub(crate) terminal: crate::custom_terminal::Terminal<B>,
    inline: Option<ratatui::layout::Rect>,
}

impl<B: ratatui::backend::Backend<Error = std::io::Error> + std::io::Write> Tui<B> {
    pub(crate) fn new(backend: B) -> std::io::Result<Self> {
        Ok(Self {
            terminal: crate::custom_terminal::Terminal::with_options(backend)?,
            inline: None,
        })
    }

    pub(crate) fn enter_alt_screen(&mut self) -> std::io::Result<()> {
        use crossterm::execute;
        execute!(
            self.terminal.backend_mut(),
            crossterm::terminal::EnterAlternateScreen
        )?;
        std::io::Write::write_all(self.terminal.backend_mut(), b"\x1b[?1007h")?;
        let size = self.terminal.size()?;
        self.inline = Some(self.terminal.viewport_area);
        self.terminal.resize(size)?;
        self.terminal
            .set_viewport_area(ratatui::layout::Rect::new(0, 0, size.width, size.height));
        self.terminal.clear()
    }

    pub(crate) fn leave_alt_screen(&mut self) -> std::io::Result<()> {
        use crossterm::execute;
        std::io::Write::write_all(self.terminal.backend_mut(), b"\x1b[?1007l")?;
        execute!(
            self.terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )?;
        if let Some(inline) = self.inline.take() {
            self.terminal.set_viewport_area(inline);
        }
        self.terminal.invalidate_viewport();
        Ok(())
    }
}

#[cfg(test)]
#[path = "tui_test_support.rs"]
pub(crate) mod test_support;
