use super::*;
use crate::paste_burst::CharDecision;
use crate::paste_burst::FlushResult;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use std::time::Instant;

impl Composer {
    pub(crate) fn flush_paste(&mut self, now: Instant) -> bool {
        let text = match self.burst.flush_if_due(now) {
            FlushResult::Paste(text) => text,
            FlushResult::Typed(ch) => ch.to_string(),
            FlushResult::None => return false,
        };
        self.editor.insert_str(&text);
        self.finish_edit();
        true
    }

    pub(crate) fn paste_pending(&self) -> bool {
        self.burst.is_active()
    }

    pub(super) fn flush_all_input(&mut self) {
        if let Some(text) = self.burst.flush_before_modified_input() {
            self.editor.insert_str(&text);
        }
        self.burst.clear_window_after_non_char();
    }

    pub(super) fn filter_paste_key(
        &mut self,
        event: KeyEvent,
        now: Instant,
    ) -> Result<bool, &'static str> {
        self.flush_paste(now);
        if !self.burst_enabled || !self.editor.allows_paste_burst() || self.chords.is_pending() {
            self.flush_all_input();
            return Ok(false);
        }
        if event.code == KeyCode::Enter && self.burst.direct_insert_newline_should_insert(now) {
            if self.editor.text().len() + self.burst.buffered_len() >= MAX_DRAFT_BYTES {
                return Err("Draft is too large.");
            }
            if !self.burst.append_newline_if_active(now) {
                self.editor.insert_str("\n");
                self.burst.extend_window(now);
            }
            return Ok(true);
        }
        if let KeyCode::Char(ch) = event.code
            && !ch.is_control()
            && (!event.modifiers.intersects(
                KeyModifiers::CONTROL
                    | KeyModifiers::ALT
                    | KeyModifiers::SUPER
                    | KeyModifiers::HYPER
                    | KeyModifiers::META,
            ) || crate::key_hint::is_altgr(event.modifiers))
        {
            if self.editor.text().len() + self.burst.buffered_len() + ch.len_utf8()
                > MAX_DRAFT_BYTES
            {
                return Err("Draft is too large.");
            }
            let decision = if ch.is_ascii() {
                Some(self.burst.on_plain_char(ch, now))
            } else {
                self.burst.on_plain_char_no_hold(now)
            };
            match decision {
                Some(CharDecision::BufferAppend | CharDecision::BeginBufferFromPending) => {
                    self.burst.append_char_to_buffer(ch, now);
                    return Ok(true);
                }
                Some(CharDecision::RetainFirstChar) => return Ok(true),
                Some(CharDecision::BeginBuffer { retro_chars }) => {
                    let cursor = self.editor.cursor();
                    if let Some(grab) = self.burst.decide_begin_buffer(
                        now,
                        &self.editor.text()[..cursor],
                        usize::from(retro_chars),
                    ) {
                        if grab.grabbed.is_empty()
                            || self.editor.retract_paste_burst(grab.start_byte)
                        {
                            self.burst.append_char_to_buffer(ch, now);
                            return Ok(true);
                        }
                        self.burst.clear_after_explicit_paste();
                    }
                }
                None => {}
            }
            return Ok(false);
        }
        self.flush_all_input();
        Ok(false)
    }
}
