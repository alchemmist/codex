use antex_core::Interaction;
use antex_core::InteractionAnswer;
use antex_core::InteractionPrompt;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::StatefulWidgetRef;
use ratatui::widgets::Widget;

use crate::textarea::TextArea;
use crate::textarea::TextAreaState;
use crate::transcript::safe_text;

pub(crate) struct Prompt {
    request: Interaction,
    selected: usize,
    answer: TextArea,
    state: TextAreaState,
}

impl Prompt {
    pub(crate) fn new(request: Interaction) -> Self {
        Self {
            request,
            selected: 0,
            answer: TextArea::new(),
            state: TextAreaState::default(),
        }
    }

    pub(crate) fn description(&self) -> Vec<Line<'static>> {
        let (title, text, choices) = match self.request.prompt() {
            InteractionPrompt::Approval { action } => ("Approval required", action, Vec::new()),
            InteractionPrompt::Question { question, choices } => {
                ("Question", question, choices.clone())
            }
        };
        let mut lines = vec![Line::from(title.magenta().bold())];
        lines.extend(
            safe_text(text)
                .lines()
                .map(|line| Line::from(line.to_owned())),
        );
        lines.extend(
            choices
                .iter()
                .enumerate()
                .map(|(index, choice)| Line::from(format!("{}. {}", index + 1, safe_text(choice)))),
        );
        lines
    }

    pub(crate) fn paste(&mut self, text: &str) -> Result<(), &'static str> {
        if !matches!(self.request.prompt(), InteractionPrompt::Question { choices, .. } if choices.is_empty())
        {
            return Err("Choose an option with arrow keys and confirm with Enter.");
        }
        if self.answer.text().len() + text.len() > antex_core::MAX_TEXT_BYTES {
            return Err("Answer is too long.");
        }
        self.answer.insert_str(text);
        Ok(())
    }

    pub(crate) fn key(&mut self, key: KeyEvent) -> Result<bool, String> {
        if key.kind == KeyEventKind::Release {
            return Ok(false);
        }
        if key.code == KeyCode::Esc {
            return Ok(true);
        }
        let choices = match self.request.prompt() {
            InteractionPrompt::Approval { .. } => vec!["Deny".to_owned(), "Allow once".to_owned()],
            InteractionPrompt::Question { choices, .. } => choices.clone(),
        };
        match key.code {
            KeyCode::Up if !choices.is_empty() => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down if !choices.is_empty() => {
                self.selected = (self.selected + 1).min(choices.len() - 1)
            }
            KeyCode::Enter => {
                let answer = match self.request.prompt() {
                    InteractionPrompt::Approval { .. } if self.selected == 1 => {
                        InteractionAnswer::AllowOnce
                    }
                    InteractionPrompt::Approval { .. } => InteractionAnswer::Deny,
                    InteractionPrompt::Question { .. } => InteractionAnswer::Text(
                        choices
                            .get(self.selected)
                            .cloned()
                            .unwrap_or_else(|| self.answer.text().to_owned()),
                    ),
                };
                self.request
                    .answer(answer)
                    .map_err(|error| error.to_string())?;
                return Ok(true);
            }
            _ if choices.is_empty() => {
                let previous = self.answer.text().to_owned();
                self.answer.input(key);
                if self.answer.text().len() > antex_core::MAX_TEXT_BYTES {
                    self.answer.set_text_clearing_elements(&previous);
                    return Err("Answer is too long.".into());
                }
            }
            _ => {}
        }
        Ok(false)
    }

    pub(crate) fn render(&mut self, area: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        let choices = match self.request.prompt() {
            InteractionPrompt::Approval { .. } => vec!["Deny".to_owned(), "Allow once".to_owned()],
            InteractionPrompt::Question { choices, .. } => choices.clone(),
        };
        if choices.is_empty() {
            StatefulWidgetRef::render_ref(&(&self.answer), area, buffer, &mut self.state);
            self.answer.cursor_pos_with_state(area, self.state)
        } else {
            let label = format!(
                "{}: {} · ↑↓ select · Enter confirm · Esc cancel",
                self.selected + 1,
                safe_text(&choices[self.selected])
            );
            Line::from(label.magenta()).render(area, buffer);
            None
        }
    }
}
