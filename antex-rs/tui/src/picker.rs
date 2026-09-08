use std::sync::Arc;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::keymap::ListAction;
use crate::keymap::RuntimeKeymap;
use crate::transcript::safe_text;

pub struct PickerItem {
    pub label: String,
    pub description: String,
    pub command: String,
}

pub struct PickerSpec {
    pub title: String,
    pub items: Vec<PickerItem>,
}

pub(crate) enum PickerAction {
    Continue,
    Cancel,
    Select(String),
}

pub(crate) struct Picker {
    spec: PickerSpec,
    query: crate::textarea::TextArea,
    selected: usize,
    keymap: Arc<RuntimeKeymap>,
    chords: crate::keymap::KeyChordMatcher,
}

impl Picker {
    pub(crate) fn new(spec: PickerSpec, keymap: Arc<RuntimeKeymap>) -> Result<Self, String> {
        let bytes = spec.title.len()
            + spec
                .items
                .iter()
                .map(|item| item.label.len() + item.description.len() + item.command.len())
                .sum::<usize>();
        if spec.title.len() > 128
            || spec.items.len() > 256
            || bytes > 256 * 1024
            || spec.items.iter().any(|item| {
                item.label.len() > 512
                    || item.description.len() > 1024
                    || item.command.len() > 4096
                    || !item.command.starts_with('/')
            })
        {
            return Err("Picker content exceeds its limits.".into());
        }
        let mut query = crate::textarea::TextArea::new();
        query.set_keymap_bindings(&keymap);
        Ok(Self {
            spec,
            query,
            selected: 0,
            keymap,
            chords: crate::keymap::KeyChordMatcher::default(),
        })
    }

    fn matches(&self) -> Vec<usize> {
        let query = self.query.text().to_lowercase();
        self.spec
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                query.split_whitespace().all(|part| {
                    item.label.to_lowercase().contains(part)
                        || item.description.to_lowercase().contains(part)
                })
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn paste(&mut self, text: &str) -> Result<(), &'static str> {
        if text.len() > 256 {
            return Err("Search input is too long.");
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if self.query.text().len() + text.len() > 256 {
            return Err("Search input is too long.");
        }
        self.query.insert_str(&text);
        self.selected = 0;
        self.chords.cancel();
        Ok(())
    }

    pub(crate) fn key(&mut self, key: KeyEvent) -> PickerAction {
        let key = match self.chords.advance(
            key,
            &self.keymap.chords,
            crate::keymap::KeymapContextSet::new(crate::keymap::KeymapContext::List),
            tokio::time::Instant::now(),
        ) {
            crate::keymap::KeyChordMatch::PassThrough => key,
            crate::keymap::KeyChordMatch::Completed(key) => key,
            crate::keymap::KeyChordMatch::Pending(_)
            | crate::keymap::KeyChordMatch::Cancelled
            | crate::keymap::KeyChordMatch::Ignored => return PickerAction::Continue,
        };
        let previous = self.query.text().to_owned();
        let cursor = self.query.cursor();
        let matches = self.matches();
        if !crate::key_hint::is_plain_text_key_event(key) {
            match self.keymap.list.action_for(key) {
                Some(ListAction::MoveUp) => self.selected = self.selected.saturating_sub(1),
                Some(ListAction::MoveDown) => {
                    self.selected = (self.selected + 1).min(matches.len().saturating_sub(1))
                }
                Some(ListAction::PageUp) => self.selected = self.selected.saturating_sub(8),
                Some(ListAction::PageDown) => {
                    self.selected = (self.selected + 8).min(matches.len().saturating_sub(1))
                }
                Some(ListAction::JumpTop) => self.selected = 0,
                Some(ListAction::JumpBottom) => self.selected = matches.len().saturating_sub(1),
                Some(ListAction::Accept) => {
                    return matches
                        .get(self.selected)
                        .map(|index| PickerAction::Select(self.spec.items[*index].command.clone()))
                        .unwrap_or(PickerAction::Continue);
                }
                Some(ListAction::Cancel) => return PickerAction::Cancel,
                Some(ListAction::MoveLeft | ListAction::MoveRight) | None => {
                    self.query.input(key);
                    self.selected = 0;
                }
            }
        } else if self.query.text().len() < 252 {
            self.query.input(key);
            self.selected = 0;
        }
        if self.query.text().len() > 256 {
            self.query.set_text_clearing_elements(&previous);
            self.query.set_cursor(cursor);
        }
        PickerAction::Continue
    }

    pub(crate) fn render(&mut self, area: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        if area.height == 0 || area.width == 0 {
            return None;
        }
        Line::from(
            format!(
                "{} · {}",
                safe_text(&self.spec.title),
                safe_text(self.query.text())
            )
            .bold(),
        )
        .render(Rect { height: 1, ..area }, buffer);
        let matches = self.matches();
        self.selected = self.selected.min(matches.len().saturating_sub(1));
        let rows = usize::from(area.height.saturating_sub(2));
        let start = self.selected.saturating_sub(rows.saturating_sub(1));
        if matches.is_empty() && area.height > 1 {
            Line::from("No matches".dim()).render(
                Rect {
                    y: area.y + 1,
                    height: 1,
                    ..area
                },
                buffer,
            );
        }
        for (row, index) in matches.iter().skip(start).take(rows).enumerate() {
            let item = &self.spec.items[*index];
            let selected = row + start == self.selected;
            let label = format!(
                "{} {}",
                if selected { "›" } else { " " },
                safe_text(&item.label)
            );
            let line = if selected {
                Line::from(label.cyan().bold())
            } else {
                Line::from(label)
            };
            line.render(
                Rect {
                    y: area.y + 1 + row as u16,
                    height: 1,
                    ..area
                },
                buffer,
            );
        }
        if area.height > 1 {
            let help = matches
                .get(self.selected)
                .map(|index| {
                    format!(
                        "{} · Enter select · Esc cancel",
                        safe_text(&self.spec.items[*index].command)
                    )
                })
                .unwrap_or_else(|| "Esc cancel".into());
            Line::from(help.dim()).render(
                Rect {
                    y: area.bottom() - 1,
                    height: 1,
                    ..area
                },
                buffer,
            );
        }
        let cursor = crate::width::display_width(&format!(
            "{} · {}",
            safe_text(&self.spec.title),
            safe_text(self.query.text())
        ))
        .min(usize::from(area.width - 1));
        Some((area.x + cursor as u16, area.y))
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
