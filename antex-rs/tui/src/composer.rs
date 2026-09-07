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
use ratatui::widgets::Widget;

use crate::editor_types::TextElement;
use crate::key_hint::KeyBindingListExt;
use crate::keymap::RuntimeKeymap;
use crate::textarea::TextArea;
use crate::textarea::TextAreaState;

const MAX_DRAFT_BYTES: usize = 256 * 1024;
const MAX_STASH_BYTES: usize = 16 * 1024 * 1024;

#[path = "composer_history.rs"]
mod history;
#[path = "composer_input.rs"]
mod input;
#[path = "composer_stash.rs"]
mod stash;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerAction {
    Submit(SubmitMode),
    Copy,
    Transcript,
    Clear,
    Interrupt,
}

pub(crate) struct Composer {
    editor: TextArea,
    state: TextAreaState,
    keymap: Arc<RuntimeKeymap>,
    images: Vec<(String, Content)>,
    stashed: Option<Draft>,
    next_image: u64,
    chords: crate::keymap::KeyChordMatcher,
    history: history::EditHistory,
    vim_start: crate::editor_types::VimModeStart,
    burst: crate::paste_burst::PasteBurst,
    burst_enabled: bool,
}

impl Composer {
    pub(crate) fn new(keymap: Arc<RuntimeKeymap>) -> Self {
        let mut editor = TextArea::new();
        editor.enable_vim_search();
        editor.set_keymap_bindings(&keymap);
        Self {
            editor,
            state: TextAreaState::default(),
            keymap,
            images: Vec::new(),
            stashed: None,
            next_image: 1,
            chords: crate::keymap::KeyChordMatcher::default(),
            history: history::EditHistory::default(),
            vim_start: crate::editor_types::VimModeStart::Normal,
            burst: crate::paste_burst::PasteBurst::default(),
            burst_enabled: true,
        }
    }

    pub(crate) fn configure(&mut self, settings: &crate::Settings) {
        self.keymap = settings.keymap.clone();
        self.editor.set_keymap_bindings(&self.keymap);
        self.editor.set_vim_mode_start(settings.vim_start);
        self.vim_start = settings.vim_start;
        self.burst_enabled = !settings.disable_paste_burst;
        self.editor.set_vim_enabled(settings.vim_mode);
        if settings.vim_start == crate::editor_types::VimModeStart::Insert {
            self.editor.enter_vim_insert_mode();
        }
        self.editor
            .set_vim_mode_indicator_enabled(settings.show_vim_mode);
        self.chords.cancel();
    }

    pub(crate) fn paste(&mut self, text: &str) -> Result<(), &'static str> {
        if let Some(query) = self.editor.vim_query_mut() {
            if query.editor.text().len() + text.len() > 256 {
                return Err("Vim search input is too long.");
            }
            query.editor.insert_str(text);
            return Ok(());
        }
        if self.editor.text().len() + self.burst.buffered_len() + text.len() > MAX_DRAFT_BYTES {
            return Err("Draft is too large; attach a file instead.");
        }
        self.begin_direct_edit();
        self.flush_all_input();
        self.burst.clear_after_explicit_paste();
        self.editor.insert_str(text);
        self.finish_edit();
        self.chords.cancel();
        Ok(())
    }

    pub(crate) fn attach(
        &mut self,
        media_type: String,
        data: Arc<[u8]>,
    ) -> Result<(), &'static str> {
        if self.search_active() {
            return Err("Close Vim search before attaching images.");
        }
        self.flush_all_input();
        self.images = self.draft().images;
        if self.images.len() >= 4 || data.len() > 3 * 1024 * 1024 || media_type.len() > 128 {
            return Err("Attach at most four normalized images, each at most 3 MiB.");
        }
        let marker = format!("[Image {}]", self.next_image);
        if self.editor.text().len() + marker.len() > MAX_DRAFT_BYTES {
            return Err("Draft is too large; attach a file instead.");
        }
        self.next_image = self
            .next_image
            .checked_add(1)
            .ok_or("image identifier exhausted")?;
        self.begin_direct_edit();
        self.editor.insert_element(&marker);
        self.images
            .push((marker, Content::Image { media_type, data }));
        self.finish_edit();
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
        self.history = history::EditHistory::default();
        match self.vim_start {
            crate::editor_types::VimModeStart::Normal => self.editor.enter_vim_normal_mode(),
            crate::editor_types::VimModeStart::Insert => self.editor.enter_vim_insert_mode(),
        }
    }

    pub(crate) fn chord_pending(&self) -> bool {
        self.chords.is_pending()
    }

    pub(crate) fn search_active(&self) -> bool {
        self.editor.vim_query().is_some()
    }

    pub(crate) fn cancel_search(&mut self) -> bool {
        let cancelled = self.editor.cancel_vim_search();
        if cancelled {
            self.finish_edit();
        }
        cancelled
    }

    pub(crate) fn key(&mut self, event: KeyEvent) -> Result<Option<ComposerAction>, &'static str> {
        self.key_at(event, std::time::Instant::now())
    }

    fn key_at(
        &mut self,
        event: KeyEvent,
        now: std::time::Instant,
    ) -> Result<Option<ComposerAction>, &'static str> {
        if event.kind == crossterm::event::KeyEventKind::Release {
            return Ok(None);
        }
        let event = if self.editor.vim_query().is_none() {
            self.editor.normalize_vim_command_event(event)
        } else {
            event
        };
        let contexts = self
            .editor
            .keymap_contexts()
            .with(crate::keymap::KeymapContext::Global)
            .with(crate::keymap::KeymapContext::Chat)
            .with(crate::keymap::KeymapContext::Composer)
            .with(self.editor.keymap_context());
        let event = match self.chords.advance(
            event,
            &self.keymap.chords,
            contexts,
            tokio::time::Instant::now(),
        ) {
            crate::keymap::KeyChordMatch::PassThrough => event,
            crate::keymap::KeyChordMatch::Completed(event) => event,
            crate::keymap::KeyChordMatch::Pending(_)
            | crate::keymap::KeyChordMatch::Cancelled
            | crate::keymap::KeyChordMatch::Ignored => return Ok(None),
        };
        if self.history_key(event) {
            return Ok(None);
        }
        self.begin_edit(event);
        if self.editor.wants_vim_search_key(event) {
            let previous = self
                .editor
                .vim_query()
                .map(|query| query.editor.text().to_owned());
            self.editor.input(event);
            if let Some(query) = self.editor.vim_query_mut()
                && query.editor.text().len() > 256
            {
                query
                    .editor
                    .set_text_clearing_elements(previous.as_deref().unwrap_or_default());
                return Err("Vim search input is too long.");
            }
            self.finish_edit();
            return Ok(None);
        }
        if self.filter_paste_key(event, now)? {
            return Ok(None);
        }
        if self.keymap.app.toggle_vim_mode.is_pressed(event) {
            self.editor.set_vim_enabled(!self.editor.is_vim_enabled());
            return Ok(None);
        }
        if self.keymap.app.copy.is_pressed(event) {
            return Ok(Some(ComposerAction::Copy));
        }
        if self.keymap.app.open_transcript.is_pressed(event) {
            return Ok(Some(ComposerAction::Transcript));
        }
        if self.keymap.app.clear_terminal.is_pressed(event) {
            return Ok(Some(ComposerAction::Clear));
        }
        if self.keymap.composer.submit.is_pressed(event) {
            return Ok(Some(ComposerAction::Submit(SubmitMode::Send)));
        }
        if self.keymap.composer.queue.is_pressed(event) {
            return Ok(Some(ComposerAction::Submit(SubmitMode::Queue)));
        }
        let before = self.draft();
        let cursor = self.editor.cursor();
        let interrupt = self.keymap.chat.interrupt_turn.is_pressed(event)
            && !self.editor.should_handle_vim_insert_escape(event);
        self.editor.input(event);
        if self.editor.text().len() > MAX_DRAFT_BYTES {
            self.editor
                .set_text_with_elements(&before.text, &before.elements);
            self.editor.set_cursor(cursor);
            return Err("Draft is too large; attach a file instead.");
        }
        self.finish_edit();
        Ok(interrupt.then_some(ComposerAction::Interrupt))
    }

    pub(crate) fn take_clipboard_yank(&mut self) -> Option<String> {
        self.editor.take_system_clipboard_yank()
    }

    pub(crate) fn height(&self, width: u16) -> u16 {
        self.editor.desired_height(width.saturating_sub(2)).max(1)
            + u16::from(self.editor.vim_query().is_some())
    }

    pub(crate) fn render(&mut self, area: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        if area.width < 3 || area.height == 0 {
            return None;
        }
        Line::from("› ".cyan()).render(Rect { width: 2, ..area }, buffer);
        let input = Rect {
            x: area.x + 2,
            width: area.width - 2,
            ..area
        };
        let body = Rect {
            height: input
                .height
                .saturating_sub(u16::from(self.editor.vim_query().is_some())),
            ..input
        };
        let highlights = self
            .editor
            .vim_search_highlights()
            .into_iter()
            .map(|range| (range, ratatui::style::Style::default().reversed().bold()))
            .collect::<Vec<_>>();
        self.editor.render_ref_styled_with_highlights(
            body,
            buffer,
            &mut self.state,
            ratatui::style::Style::default(),
            &highlights,
        );
        if let Some(query) = self.editor.vim_query() {
            let query_area = Rect {
                y: input.bottom().saturating_sub(1),
                height: 1,
                ..input
            };
            query.render(query_area, buffer);
            return query.cursor_pos(query_area);
        }
        if self.editor.is_empty() {
            Line::from("Ask Antex to do anything".dim()).render(input, buffer);
        }
        self.editor.cursor_pos_with_state(input, self.state)
    }

    pub(crate) fn mode_label(&self) -> Option<&'static str> {
        self.editor.vim_mode_indicator_span()?;
        self.editor.vim_mode_label()
    }
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "composer_vim_tests.rs"]
mod vim_tests;

#[cfg(test)]
#[path = "composer_input_tests.rs"]
mod input_tests;
