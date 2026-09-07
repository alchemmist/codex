use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub(crate) enum Overlay {
    Prompt(crate::prompt::Prompt),
    Picker(crate::picker::Picker),
}

pub(crate) enum OverlayAction {
    Continue,
    Close,
    Command(String),
}

impl Overlay {
    pub(crate) fn height(&self) -> u16 {
        match self {
            Self::Prompt(_) => 2,
            Self::Picker(_) => 10,
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent) -> Result<OverlayAction, String> {
        match self {
            Self::Prompt(prompt) => prompt.key(key).map(|closed| {
                if closed {
                    OverlayAction::Close
                } else {
                    OverlayAction::Continue
                }
            }),
            Self::Picker(picker) => Ok(match picker.key(key) {
                crate::picker::PickerAction::Continue => OverlayAction::Continue,
                crate::picker::PickerAction::Cancel => OverlayAction::Close,
                crate::picker::PickerAction::Select(command) => OverlayAction::Command(command),
            }),
        }
    }

    pub(crate) fn paste(&mut self, text: &str) -> Result<(), &'static str> {
        match self {
            Self::Prompt(prompt) => prompt.paste(text),
            Self::Picker(picker) => picker.paste(text),
        }
    }

    pub(crate) fn render(&mut self, area: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        match self {
            Self::Prompt(prompt) => prompt.render(area, buffer),
            Self::Picker(picker) => picker.render(area, buffer),
        }
    }
}
