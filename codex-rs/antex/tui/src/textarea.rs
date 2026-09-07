//! The textarea owns editable composer text, placeholder elements, cursor/wrap state, and a
//! single-entry kill buffer.
//!
//! Whole-buffer replacement APIs intentionally rebuild only the visible draft state. They clear
//! element ranges and derived cursor/wrapping caches, but they keep the kill buffer intact so a
//! caller can clear or rewrite the draft and still allow `Ctrl+Y` to restore the user's most
//! recent `Ctrl+K`. This is the contract higher-level composer flows rely on after submit,
//! slash-command dispatch, and other synthetic clears.
//!
//! This module does not implement an Emacs-style multi-entry kill ring. It keeps only the most
//! recent killed span.
//!
//! Wrapping also reserves a visible insertion point: full logical lines get continuation rows,
//! and trailing spaces wrap instead of moving the cursor outside the textarea. At soft word
//! breaks, interior separators hang off the preceding row without changing the editable text.
//! Visible web URLs carry their complete terminal hyperlink destination across wrapped rows;
//! masked rendering never exposes hyperlink destinations.

use crate::editor_types::ByteRange;
use crate::editor_types::TextElement as UserTextElement;
use crate::editor_types::VimModeStart;
use crate::key_hint::KeyBindingListExt;
use crate::key_hint::is_altgr;
use crate::keymap::EditorKeymap;
use crate::keymap::KeymapContext;
use crate::keymap::RuntimeKeymap;
use crate::keymap::VimNormalKeymap;
use crate::keymap::VimOperatorKeymap;
use crate::keymap::VimSearchKeymap;
use crate::keymap::VimTextObjectKeymap;
use crate::width::display_width;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::widgets::StatefulWidgetRef;
use ratatui::widgets::WidgetRef;
use std::borrow::Cow;
use std::cell::OnceCell;
use std::cell::Ref;
use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

#[path = "textarea/editing.rs"]
mod editing;
#[path = "textarea/elements.rs"]
mod elements;
#[path = "textarea/hyperlinks.rs"]
mod hyperlinks;
#[path = "textarea/mode.rs"]
mod mode;
#[path = "textarea/navigation.rs"]
mod navigation;
#[path = "textarea/painting.rs"]
mod painting;
#[path = "textarea/russian_layout.rs"]
mod russian_layout;
#[path = "textarea/vim.rs"]
mod vim;
#[path = "textarea/vim_commands.rs"]
mod vim_commands;
#[path = "textarea/vim_input.rs"]
mod vim_input;
#[path = "textarea/vim_search.rs"]
mod vim_search;
#[path = "textarea/visual.rs"]
mod visual;
#[path = "textarea/wrapping.rs"]
mod wrapping;
use self::vim::VimMode;
use self::vim::VimMotion;
use self::vim::VimOperator;
use self::vim::VimPending;
use self::vim::VimTextObject;
use self::vim::VimTextObjectScope;
use self::vim_commands::VimAction;
use self::vim_commands::VimCommandState;
use self::vim_commands::VimEditTarget;
use self::vim_commands::VimInsertPosition;
pub(crate) use self::vim_commands::VimPersistentState;
use self::visual::VimVisualKind;
use self::visual::VimVisualState;
use self::visual::visual_control_key;
use self::visual::visual_key;
use self::visual::visual_shift_key;

const WORD_SEPARATORS: &str = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";

fn is_word_separator(ch: char) -> bool {
    WORD_SEPARATORS.contains(ch)
}

fn split_word_pieces(run: &str) -> Vec<(usize, &str)> {
    let mut pieces = Vec::new();
    for (segment_start, segment) in run.split_word_bound_indices() {
        let mut piece_start = 0;
        let mut chars = segment.char_indices();
        let Some((_, first_char)) = chars.next() else {
            continue;
        };
        let mut in_separator = is_word_separator(first_char);

        for (idx, ch) in chars {
            let is_separator = is_word_separator(ch);
            if is_separator == in_separator {
                continue;
            }
            pieces.push((segment_start + piece_start, &segment[piece_start..idx]));
            piece_start = idx;
            in_separator = is_separator;
        }

        pieces.push((segment_start + piece_start, &segment[piece_start..]));
    }

    pieces
}

/// Replace tabs with the one-column representation used for rendering and wrapping.
///
/// A tab and a space are both one byte, so ranges computed from this text still index the original
/// editable text.
fn text_for_display(text: &str) -> Cow<'_, str> {
    if text.contains('\t') {
        Cow::Owned(text.replace('\t', " "))
    } else {
        Cow::Borrowed(text)
    }
}

#[derive(Debug, Clone)]
struct TextElement {
    id: u64,
    range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextElementSnapshot {
    pub(crate) id: u64,
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
}

/// `TextArea` is the editable buffer behind the TUI composer.
///
/// It owns the raw UTF-8 text, placeholder-like text elements that must move atomically with
/// edits, cursor/wrapping state for rendering, and a single-entry kill buffer for `Ctrl+K` /
/// `Ctrl+Y` style editing. Callers may replace the entire visible buffer through
/// [`Self::set_text_clearing_elements`] or [`Self::set_text_with_elements`] without disturbing the
/// kill buffer; if they incorrectly assume those methods fully reset editing state, a later yank
/// will appear to restore stale text from the user's perspective.
#[derive(Debug)]
pub(crate) struct TextArea {
    text: String,
    cursor_pos: usize,
    wrap_cache: RefCell<Option<WrapCache>>,
    preferred_col: Option<usize>,
    elements: Vec<TextElement>,
    next_element_id: u64,
    kill_buffer: String,
    kill_buffer_kind: KillBufferKind,
    vim_enabled: bool,
    vim_mode_start: VimModeStart,
    vim_mode_indicator_enabled: bool,
    vim_mode: VimMode,
    vim_visual: Option<VimVisualState>,
    vim_pending: VimPending,
    vim_commands: VimCommandState,
    pending_system_clipboard_yank: Option<String>,
    vim_search: vim_search::VimSearch,
    vim_search_enabled: bool,
    editor_keymap: Arc<EditorKeymap>,
    vim_normal_keymap: VimNormalKeymap,
    vim_operator_keymap: VimOperatorKeymap,
    vim_search_keymap: VimSearchKeymap,
    vim_text_object_keymap: VimTextObjectKeymap,
}

#[derive(Debug, Clone)]
struct WrapCache {
    width: u16,
    lines: Vec<Range<usize>>,
    hyperlinks: OnceCell<hyperlinks::HyperlinkCache>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TextAreaState {
    /// Index into wrapped lines of the first visible line.
    scroll: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KillBufferKind {
    /// Characterwise kills and yanks paste at the cursor.
    Characterwise,
    /// Linewise kills and yanks paste as whole lines below the cursor line.
    Linewise,
}

impl TextArea {
    pub fn new() -> Self {
        let defaults = RuntimeKeymap::defaults();
        Self {
            text: String::new(),
            cursor_pos: 0,
            wrap_cache: RefCell::new(None),
            preferred_col: None,
            elements: Vec::new(),
            next_element_id: 1,
            kill_buffer: String::new(),
            kill_buffer_kind: KillBufferKind::Characterwise,
            vim_enabled: false,
            vim_mode_start: VimModeStart::Normal,
            vim_mode_indicator_enabled: false,
            vim_mode: VimMode::Insert,
            vim_visual: None,
            vim_pending: VimPending::None,
            vim_commands: VimCommandState::default(),
            pending_system_clipboard_yank: None,
            vim_search: vim_search::VimSearch::default(),
            vim_search_enabled: false,
            editor_keymap: defaults.editor,
            vim_normal_keymap: defaults.vim_normal,
            vim_operator_keymap: defaults.vim_operator,
            vim_search_keymap: defaults.vim_search,
            vim_text_object_keymap: defaults.vim_text_object,
        }
    }

    /// Replace the editor and Vim keymaps used by subsequent text-editing input.
    ///
    /// This method intentionally swaps only the keymap caches. It does not
    /// reinterpret pending input, change Vim mode, move the cursor, or mutate
    /// the kill buffer, so callers can safely apply a live config update while
    /// preserving the current draft exactly as typed.
    pub fn set_keymap_bindings(&mut self, keymap: &RuntimeKeymap) {
        self.editor_keymap = Arc::clone(&keymap.editor);
        self.vim_normal_keymap = keymap.vim_normal.clone();
        self.vim_operator_keymap = keymap.vim_operator.clone();
        self.vim_search_keymap = keymap.vim_search.clone();
        self.vim_text_object_keymap = keymap.vim_text_object.clone();
    }

    /// Replace the visible textarea text and clear any existing text elements.
    ///
    /// This is the "fresh buffer" path for callers that want plain text with no placeholder
    /// ranges. It intentionally preserves the current kill buffer, because higher-level flows such
    /// as submit or slash-command dispatch clear the draft through this method and still want
    /// `Ctrl+Y` to recover the user's most recent kill.
    pub fn set_text_clearing_elements(&mut self, text: &str) {
        self.set_text_inner(text, /*elements*/ None);
    }

    /// Replace the visible textarea text and rebuild the provided text elements.
    ///
    /// As with [`Self::set_text_clearing_elements`], this resets only state derived from the
    /// visible buffer. The kill buffer survives so callers restoring drafts or external edits do
    /// not silently discard a pending yank target.
    pub fn set_text_with_elements(&mut self, text: &str, elements: &[UserTextElement]) {
        self.set_text_inner(text, Some(elements));
    }

    fn set_text_inner(&mut self, text: &str, elements: Option<&[UserTextElement]>) {
        // Stage 1: replace the raw text and keep the cursor in a safe byte range.
        self.text = text.to_string();
        self.cursor_pos = self.cursor_pos.clamp(0, self.text.len());
        // Stage 2: rebuild element ranges from scratch against the new text.
        self.elements.clear();
        if let Some(elements) = elements {
            for elem in elements {
                let mut start = elem.byte_range.start.min(self.text.len());
                let mut end = elem.byte_range.end.min(self.text.len());
                start = self.clamp_pos_to_char_boundary(start);
                end = self.clamp_pos_to_char_boundary(end);
                if start >= end {
                    continue;
                }
                let id = self.next_element_id();
                self.elements.push(TextElement {
                    id,
                    range: start..end,
                });
            }
            self.elements.sort_by_key(|e| e.range.start);
        }
        // Stage 3: clamp the cursor and reset derived state tied to the prior content.
        // The kill buffer is editing history rather than visible-buffer state, so full-buffer
        // replacements intentionally leave it alone.
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
        self.wrap_cache.replace(None);
        self.preferred_col = None;
        self.vim_pending = VimPending::None;
        self.vim_visual = None;
        self.vim_search = vim_search::VimSearch::default();
        self.vim_commands = VimCommandState::default();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn insert_str(&mut self, text: &str) {
        self.record_vim_inserted_text(text);
        if self.is_vim_replace_mode() {
            self.replace_vim_text(text);
        } else {
            self.insert_str_at(self.cursor_pos, text);
        }
    }

    pub fn insert_str_at(&mut self, pos: usize, text: &str) {
        self.clear_vim_replace_recovery();
        let pos = self.clamp_pos_for_insertion(pos);
        self.text.insert_str(pos, text);
        self.wrap_cache.replace(None);
        if pos <= self.cursor_pos {
            self.cursor_pos += text.len();
        }
        self.shift_elements(pos, /*removed*/ 0, text.len());
        self.preferred_col = None;
    }

    pub fn replace_range(&mut self, range: std::ops::Range<usize>, text: &str) {
        self.clear_vim_replace_recovery();
        self.replace_range_preserving_recovery(range, text);
    }

    // Replace typing and Backspace keep contiguous recovery, but still respect atomic elements.
    fn replace_range_preserving_recovery(&mut self, range: Range<usize>, text: &str) {
        let range = self.expand_range_to_element_boundaries(range);
        self.replace_range_raw(range, text);
    }

    fn replace_range_raw(&mut self, range: std::ops::Range<usize>, text: &str) {
        assert!(range.start <= range.end);
        let start = range.start.clamp(0, self.text.len());
        let end = range.end.clamp(0, self.text.len());
        let removed_len = end - start;
        let inserted_len = text.len();
        if removed_len == 0 && inserted_len == 0 {
            return;
        }
        let diff = inserted_len as isize - removed_len as isize;

        self.text.replace_range(range, text);
        self.wrap_cache.replace(None);
        self.preferred_col = None;
        self.update_elements_after_replace(start, end, inserted_len);

        // Update the cursor position to account for the edit.
        self.cursor_pos = if self.cursor_pos < start {
            // Cursor was before the edited range – no shift.
            self.cursor_pos
        } else if self.cursor_pos <= end {
            // Cursor was inside the replaced range – move to end of the new text.
            start + inserted_len
        } else {
            // Cursor was after the replaced range – shift by the length diff.
            ((self.cursor_pos as isize) + diff) as usize
        }
        .min(self.text.len());

        // Ensure cursor is not inside an element
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
    }

    pub fn cursor(&self) -> usize {
        self.cursor_pos
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor_pos = pos.clamp(0, self.text.len());
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
        self.preferred_col = None;
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        self.wrapped_lines(width).len() as u16
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.cursor_pos_with_state(area, TextAreaState::default())
    }

    /// Returns an on-screen cursor position within `area`, accounting for wrapping and scrolling.
    ///
    /// Returns `None` when the viewport has no visible cells.
    pub fn cursor_pos_with_state(&self, area: Rect, state: TextAreaState) -> Option<(u16, u16)> {
        if area.is_empty() {
            return None;
        }

        let lines = self.wrapped_lines(area.width);
        let effective_scroll = self.effective_scroll(area, &lines, state.scroll);
        let (i, col) = wrapping::cursor_position(&self.text, &lines, area.width, self.cursor_pos)?;
        let screen_row = i
            .saturating_sub(effective_scroll as usize)
            .try_into()
            .unwrap_or(0);
        Some((area.x + col as u16, area.y + screen_row))
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn current_display_col(&self) -> usize {
        let bol = self.beginning_of_current_line();
        display_width(&self.text[bol..self.cursor_pos])
    }

    fn move_to_display_col_on_line(
        &mut self,
        line_start: usize,
        line_end: usize,
        target_col: usize,
    ) {
        let mut width_so_far = 0usize;
        for (i, g) in self.text[line_start..line_end].grapheme_indices(true) {
            width_so_far += display_width(g);
            if width_so_far > target_col {
                self.cursor_pos = line_start + i;
                // Avoid landing inside an element; round to nearest boundary
                self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
                return;
            }
        }
        self.cursor_pos = line_end;
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
    }

    fn beginning_of_line(&self, pos: usize) -> usize {
        self.text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0)
    }
    fn beginning_of_current_line(&self) -> usize {
        self.beginning_of_line(self.cursor_pos)
    }

    fn first_non_blank_of_current_line(&self) -> usize {
        let bol = self.beginning_of_current_line();
        let eol = self.end_of_current_line();
        self.text[bol..eol]
            .char_indices()
            .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(bol + offset))
            .unwrap_or(eol)
    }

    fn end_of_line(&self, pos: usize) -> usize {
        self.text[pos..]
            .find('\n')
            .map(|i| i + pos)
            .unwrap_or(self.text.len())
    }
    fn end_of_current_line(&self) -> usize {
        self.end_of_line(self.cursor_pos)
    }

    pub fn input(&mut self, event: KeyEvent) {
        // Only process key presses or repeats; ignore releases to avoid inserting
        // characters on key-up events when modifiers are no longer reported.
        if !matches!(event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }
        if self.vim_enabled {
            self.handle_vim_input(event);
        } else {
            let keymap = self.editor_keymap.clone();
            self.input_with_keymap(event, &keymap);
        }
    }

    pub fn input_with_keymap(&mut self, event: KeyEvent, keymap: &EditorKeymap) {
        if keymap.insert_newline.is_pressed(event) {
            self.insert_str("\n");
            return;
        }

        if keymap.delete_backward_word.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::DeleteBackwardWord);
            return;
        }

        // Windows AltGr generates ALT|CONTROL. Preserve typed characters for AltGr users
        // unless a specific shortcut already matched above.
        if let KeyEvent {
            code: KeyCode::Char(c),
            modifiers,
            ..
        } = event
            && is_altgr(modifiers)
        {
            self.insert_str(&c.to_string());
            return;
        }

        if keymap.delete_backward.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::DeleteBackward);
            return;
        }
        if keymap.delete_forward_word.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::DeleteForwardWord);
            return;
        }
        if keymap.delete_forward.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::DeleteForward);
            return;
        }
        if keymap.kill_line_start.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::KillLineStart);
            return;
        }
        if keymap.kill_whole_line.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::KillLine);
            return;
        }
        if keymap.kill_line_end.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::KillLineEnd);
            return;
        }
        if keymap.yank.is_pressed(event) {
            self.yank();
            return;
        }
        if keymap.move_word_left.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::MoveWordLeft);
            return;
        }
        if keymap.move_word_right.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::MoveWordRight);
            return;
        }
        if keymap.move_left.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::MoveLeft);
            return;
        }
        if keymap.move_right.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::MoveRight);
            return;
        }
        if keymap.move_up.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::MoveUp);
            return;
        }
        if keymap.move_down.is_pressed(event) {
            self.apply_vim_insert_action(VimAction::MoveDown);
            return;
        }
        if keymap.move_line_start.is_pressed(event) {
            let move_up_at_bol = matches!(
                event,
                KeyEvent {
                    code: KeyCode::Char('a'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }
            );
            self.apply_vim_insert_action(VimAction::MoveLineStart { move_up_at_bol });
            return;
        }
        if keymap.move_line_end.is_pressed(event) {
            let move_down_at_eol = matches!(
                event,
                KeyEvent {
                    code: KeyCode::Char('e'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }
            );
            self.apply_vim_insert_action(VimAction::MoveLineEnd { move_down_at_eol });
            return;
        }

        if let KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        } = event
        {
            // Insert plain characters (and Shift-modified). Do not insert when ALT is held,
            // because many terminals map Option/Meta combos to ALT+<char>.
            if c.is_ascii_control() {
                return;
            }
            self.insert_str(&c.to_string());
        }

        tracing::debug!("Unhandled key event in TextArea: {:?}", event);
    }

    /// Returns cached grapheme-safe visual ranges, including cursor-position sentinel bytes.
    ///
    /// Overflowing spaces wrap without separating a partial whitespace continuation from the next
    /// word, existing word breakpoints stay intact, and full logical lines receive a continuation
    /// row so their insertion point stays visible.
    #[expect(clippy::unwrap_used)]
    fn wrapped_lines(&self, width: u16) -> Ref<'_, Vec<Range<usize>>> {
        // Ensure cache is ready (potentially mutably borrow, then drop)
        {
            let mut cache = self.wrap_cache.borrow_mut();
            let needs_recalc = match cache.as_ref() {
                Some(c) => c.width != width,
                None => true,
            };
            if needs_recalc {
                let display_text = text_for_display(&self.text);
                let lines = wrapping::wrapped_lines(display_text.as_ref(), width);
                *cache = Some(WrapCache {
                    width,
                    lines,
                    hyperlinks: OnceCell::new(),
                });
            }
        }

        let cache = self.wrap_cache.borrow();
        Ref::map(cache, |c| &c.as_ref().unwrap().lines)
    }

    /// Calculate the scroll offset that should be used to satisfy the
    /// invariants given the current area size and wrapped lines.
    ///
    /// - Cursor is always on screen.
    /// - No scrolling if content fits in the area.
    fn effective_scroll(&self, area: Rect, lines: &[Range<usize>], current_scroll: u16) -> u16 {
        let total_lines = lines.len() as u16;
        if area.height >= total_lines {
            return 0;
        }

        let cursor_line_idx =
            wrapping::cursor_position(&self.text, lines, area.width, self.cursor_pos)
                .map_or(0, |(row, _)| row) as u16;

        let max_scroll = total_lines.saturating_sub(area.height);
        let mut scroll = current_scroll.min(max_scroll);

        // Ensure cursor is visible within [scroll, scroll + area_height)
        if cursor_line_idx < scroll {
            scroll = cursor_line_idx;
        } else if cursor_line_idx >= scroll + area.height {
            scroll = cursor_line_idx + 1 - area.height;
        }
        scroll
    }
}

impl WidgetRef for &TextArea {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.wrapped_lines(area.width);
        self.render_lines(
            area,
            buf,
            &lines,
            0..lines.len().min(usize::from(area.height)),
            Style::default(),
            &[],
        );
    }
}

impl StatefulWidgetRef for &TextArea {
    type State = TextAreaState;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let lines = self.wrapped_lines(area.width);
        let scroll = self.effective_scroll(area, &lines, state.scroll);
        state.scroll = scroll;

        let start = scroll as usize;
        let end = (scroll + area.height).min(lines.len() as u16) as usize;
        self.render_lines(area, buf, &lines, start..end, Style::default(), &[]);
    }
}

#[cfg(test)]
#[path = "textarea_tests.rs"]
mod tests;
