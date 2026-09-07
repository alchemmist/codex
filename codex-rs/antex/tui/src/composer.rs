use std::sync::Arc;

use antex_core::Content;
use antex_core::MAX_TEXT_BYTES;
use antex_core::ToolScope;
use antex_core::UserInput;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::StatefulWidgetRef;
use ratatui::widgets::Widget;

use crate::editor_types::TextElement;
use crate::key_hint::KeyBindingListExt;
use crate::keymap::RuntimeKeymap;
use crate::textarea::TextArea;
use crate::textarea::TextAreaState;

const MAX_DRAFT_BYTES: usize = 256 * 1024;
const MAX_STASHES: usize = 8;
const MAX_STASH_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Draft {
    text: String,
    elements: Vec<TextElement>,
    images: Vec<(String, Content)>,
}

impl Draft {
    fn size(&self) -> usize {
        self.text.len()
            + self
                .images
                .iter()
                .map(|(_, image)| match image {
                    Content::Image { data, .. } => data.len(),
                    Content::Text(_) | Content::Reasoning(_) | Content::Continuation { .. } => 0,
                })
                .sum::<usize>()
    }

    pub(crate) fn input(&self) -> UserInput {
        let mut text = self.text.clone();
        for element in self.elements.iter().rev() {
            if element
                .placeholder(&self.text)
                .is_some_and(|marker| self.images.iter().any(|(name, _)| name == marker))
            {
                text.replace_range(element.byte_range.start..element.byte_range.end, "");
            }
        }
        let mut remaining = text.as_str();
        let mut content = Vec::new();
        while !remaining.is_empty() {
            let mut end = remaining.len().min(MAX_TEXT_BYTES);
            while !remaining.is_char_boundary(end) {
                end -= 1;
            }
            content.push(Content::Text(remaining[..end].to_owned()));
            remaining = &remaining[end..];
        }
        content.extend(self.images.iter().map(|(_, image)| image.clone()));
        UserInput {
            content,
            tool_scope: ToolScope::Default,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubmitMode {
    Send,
    Queue,
}

pub(crate) struct Composer {
    editor: TextArea,
    state: TextAreaState,
    keymap: Arc<RuntimeKeymap>,
    images: Vec<(String, Content)>,
    stashes: Vec<Draft>,
    next_image: u64,
}

impl Composer {
    pub(crate) fn new(keymap: Arc<RuntimeKeymap>) -> Self {
        let mut editor = TextArea::new();
        editor.set_keymap_bindings(&keymap);
        Self {
            editor,
            state: TextAreaState::default(),
            keymap,
            images: Vec::new(),
            stashes: Vec::new(),
            next_image: 1,
        }
    }

    pub(crate) fn paste(&mut self, text: &str) -> Result<(), &'static str> {
        if self.editor.text().len() + text.len() > MAX_DRAFT_BYTES {
            return Err("Draft is too large; attach a file instead.");
        }
        self.editor.insert_str(text);
        Ok(())
    }

    pub(crate) fn attach(
        &mut self,
        media_type: String,
        data: Arc<[u8]>,
    ) -> Result<(), &'static str> {
        self.images = self.draft().images;
        if self.images.len() >= 4 || data.len() > 3 * 1024 * 1024 || media_type.len() > 128 {
            return Err("Attach at most four normalized images, each at most 3 MiB.");
        }
        let marker = format!("[Image {}]", self.next_image);
        if self.editor.text().len() + marker.len() > MAX_DRAFT_BYTES {
            return Err("Draft is too large; attach a file instead.");
        }
        self.next_image += 1;
        self.editor.insert_element(&marker);
        self.images
            .push((marker, Content::Image { media_type, data }));
        Ok(())
    }

    pub(crate) fn draft(&self) -> Draft {
        let payloads = self.editor.element_payloads();
        Draft {
            text: self.editor.text().to_owned(),
            elements: self.editor.text_elements(),
            images: self
                .images
                .iter()
                .filter(|(marker, _)| payloads.contains(marker))
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn accept_submission(&mut self) {
        self.editor.set_text_clearing_elements("");
        self.images.clear();
        self.state = TextAreaState::default();
    }

    pub(crate) fn stash(&mut self) -> Result<(), &'static str> {
        let draft = self.draft();
        if draft.text.is_empty() {
            return Ok(());
        }
        if self.stashes.len() == MAX_STASHES
            || self.stashes.iter().map(Draft::size).sum::<usize>() + draft.size() > MAX_STASH_BYTES
        {
            return Err("Prompt stash is full; restore a saved draft first.");
        }
        self.stashes.push(draft);
        self.accept_submission();
        Ok(())
    }

    pub(crate) fn restore_stash(&mut self) -> Result<(), &'static str> {
        if !self.editor.is_empty() {
            return Err("Save or clear the current draft before restoring a stash.");
        }
        let Some(draft) = self.stashes.pop() else {
            return Err("Prompt stash is empty.");
        };
        self.editor
            .set_text_with_elements(&draft.text, &draft.elements);
        self.images = draft.images;
        self.state = TextAreaState::default();
        Ok(())
    }

    pub(crate) fn key(&mut self, event: KeyEvent) -> Result<Option<SubmitMode>, &'static str> {
        if self.keymap.app.toggle_vim_mode.is_pressed(event) {
            self.editor.set_vim_enabled(!self.editor.is_vim_enabled());
            return Ok(None);
        }
        if self.keymap.composer.submit.is_pressed(event) {
            return Ok(Some(SubmitMode::Send));
        }
        if self.keymap.composer.queue.is_pressed(event) {
            return Ok(Some(SubmitMode::Queue));
        }
        let before = self.draft();
        let cursor = self.editor.cursor();
        self.editor.input(event);
        if self.editor.text().len() > MAX_DRAFT_BYTES {
            self.editor
                .set_text_with_elements(&before.text, &before.elements);
            self.editor.set_cursor(cursor);
            return Err("Draft is too large; attach a file instead.");
        }
        Ok(None)
    }

    pub(crate) fn take_clipboard_yank(&mut self) -> Option<String> {
        self.editor.take_system_clipboard_yank()
    }

    pub(crate) fn height(&self, width: u16) -> u16 {
        self.editor.desired_height(width.saturating_sub(2)).max(1)
    }

    pub(crate) fn render(&mut self, area: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        if area.width < 3 || area.height == 0 {
            return None;
        }
        Line::from("› ".yellow()).render(Rect { width: 2, ..area }, buffer);
        let input = Rect {
            x: area.x + 2,
            width: area.width - 2,
            ..area
        };
        StatefulWidgetRef::render_ref(&(&self.editor), input, buffer, &mut self.state);
        if self.editor.is_empty() {
            Line::from("Ask Antex to do anything".dim()).render(input, buffer);
        }
        self.editor.cursor_pos_with_state(input, self.state)
    }

    pub(crate) fn mode_label(&self) -> Option<&'static str> {
        self.editor.vim_mode_label()
    }
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;
