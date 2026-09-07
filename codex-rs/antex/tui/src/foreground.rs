use std::collections::VecDeque;
use std::future::Future;
use std::io;

use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use futures::Stream;
use futures::StreamExt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputState {
    Open,
    Closed,
}

pub(crate) async fn wait<F, T, E>(
    future: F,
    input: &mut E,
    buffered: &mut VecDeque<io::Result<Event>>,
    state: &mut InputState,
) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
    E: Stream<Item = io::Result<Event>> + Unpin,
{
    tokio::pin!(future);
    let mut bytes = 0;
    loop {
        tokio::select! {
            result = &mut future => return result,
            event = input.next() => {
                let Some(event) = event else { *state = InputState::Closed; return Err("Terminal input closed.".into()); };
                if let Ok(Event::Key(key)) = &event
                    && key.kind != KeyEventKind::Release
                    && (key.code == KeyCode::Esc || key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                    return Err("Cancelled.".into());
                }
                bytes += match &event { Ok(Event::Paste(text)) => text.len(), _ => 1 };
                buffered.push_back(event);
                if buffered.len() >= 256 || bytes > 256 * 1024 { return Err("Command cancelled: buffered input limit reached.".into()); }
            }
        }
    }
}

#[cfg(test)]
#[path = "foreground_tests.rs"]
mod tests;
