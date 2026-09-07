use super::*;

impl TextArea {
    // ####### Input Functions #######
    pub fn delete_backward(&mut self, n: usize) {
        if n == 0 || self.cursor_pos == 0 {
            return;
        }
        let mut target = self.cursor_pos;
        for _ in 0..n {
            if let Some((boundary, ch)) = self.text[..target].char_indices().next_back()
                && matches!(
                    ch,
                    // Thai-only special casing is not ideal; refactor if it becomes more complex.
                    // All Thai nonspacing marks: vowel signs, tone marks, and other diacritics.
                    '\u{0e31}' | '\u{0e34}'..='\u{0e3a}' | '\u{0e47}'..='\u{0e4e}'
                )
                && !self
                    .elements
                    .iter()
                    .any(|element| target > element.range.start && target <= element.range.end)
            {
                target = boundary;
            } else {
                target = self.prev_atomic_boundary(target);
            }
            if target == 0 {
                break;
            }
        }
        self.replace_range(target..self.cursor_pos, "");
    }

    pub fn delete_forward(&mut self, n: usize) {
        if n == 0 || self.cursor_pos >= self.text.len() {
            return;
        }
        let mut target = self.cursor_pos;
        for _ in 0..n {
            target = self.next_atomic_boundary(target);
            if target >= self.text.len() {
                break;
            }
        }
        self.replace_range(self.cursor_pos..target, "");
    }

    pub fn delete_forward_kill(&mut self, n: usize) {
        if n == 0 || self.cursor_pos >= self.text.len() {
            return;
        }
        let mut target = self.cursor_pos;
        for _ in 0..n {
            target = self.next_atomic_boundary(target);
            if target >= self.text.len() {
                break;
            }
        }
        self.kill_range(self.cursor_pos..target);
    }

    pub fn delete_backward_word(&mut self) {
        let start = self.beginning_of_previous_word();
        self.kill_range(start..self.cursor_pos);
    }

    /// Delete text to the right of the cursor using "word" semantics.
    ///
    /// Deletes from the current cursor position through the end of the next word as determined
    /// by `end_of_next_word()`. Any whitespace (including newlines) between the cursor and that
    /// word is included in the deletion.
    pub fn delete_forward_word(&mut self) {
        let end = self.end_of_next_word();
        if end > self.cursor_pos {
            self.kill_range(self.cursor_pos..end);
        }
    }

    /// Kill from the cursor to the end of the current logical line.
    ///
    /// If the cursor is already at end-of-line and a trailing newline exists, this kills that
    /// newline so repeated invocations continue making progress. The removed text becomes the next
    /// yank target and remains available even if a caller later clears or rewrites the visible
    /// buffer via `set_text_*`.
    pub fn kill_to_end_of_line(&mut self) {
        let eol = self.end_of_current_line();
        let range = if self.cursor_pos == eol {
            if eol < self.text.len() {
                Some(self.cursor_pos..eol + 1)
            } else {
                None
            }
        } else {
            Some(self.cursor_pos..eol)
        };

        if let Some(range) = range {
            self.kill_range(range);
        }
    }

    pub(super) fn vim_kill_to_end_of_line(&mut self) {
        let eol = self.end_of_current_line();
        if self.cursor_pos < eol {
            self.kill_range(self.cursor_pos..eol);
        }
    }

    pub fn kill_to_beginning_of_line(&mut self) {
        let bol = self.beginning_of_current_line();
        let range = if self.cursor_pos == bol {
            if bol > 0 { Some(bol - 1..bol) } else { None }
        } else {
            Some(bol..self.cursor_pos)
        };

        if let Some(range) = range {
            self.kill_range(range);
        }
    }

    /// Insert the most recently killed text at the cursor.
    ///
    /// This uses the textarea's single-entry kill buffer. Because whole-buffer replacement APIs do
    /// not clear that buffer, `yank` can restore text after composer-level clears such as submit
    /// and slash-command dispatch.
    pub fn yank(&mut self) {
        if self.kill_buffer.is_empty() {
            return;
        }
        let text = self.kill_buffer.clone();
        self.insert_str(&text);
    }

    pub(super) fn kill_range(&mut self, range: Range<usize>) {
        self.kill_range_with_kind(range, KillBufferKind::Characterwise);
    }

    pub(super) fn kill_line_range(&mut self, range: Range<usize>) {
        self.kill_range_with_kind(range, KillBufferKind::Linewise);
    }

    pub(super) fn kill_range_with_kind(&mut self, range: Range<usize>, kind: KillBufferKind) {
        let range = self.expand_range_to_element_boundaries(range);
        if range.start >= range.end {
            return;
        }

        let removed = self.text[range.clone()].to_string();
        if removed.is_empty() {
            return;
        }

        self.store_kill_buffer(removed, kind);
        self.replace_range_raw(range, "");
    }

    pub(super) fn yank_range(&mut self, range: Range<usize>) {
        self.yank_range_with_kind(range, KillBufferKind::Characterwise);
    }

    pub(super) fn yank_line_range(&mut self, range: Range<usize>) {
        self.yank_range_with_kind(range, KillBufferKind::Linewise);
    }

    pub(super) fn yank_range_with_kind(&mut self, range: Range<usize>, kind: KillBufferKind) {
        let range = self.expand_range_to_element_boundaries(range);
        if range.start >= range.end {
            return;
        }
        let removed = self.text[range].to_string();
        if removed.is_empty() {
            return;
        }
        self.store_kill_buffer(removed, kind);
    }

    pub(super) fn store_kill_buffer(&mut self, text: String, kind: KillBufferKind) {
        self.kill_buffer = text;
        self.kill_buffer_kind = kind;
    }

    pub(super) fn paste_after_cursor(&mut self) {
        if self.kill_buffer.is_empty() {
            return;
        }
        if self.kill_buffer_kind == KillBufferKind::Linewise {
            self.paste_line_after_current_line();
            return;
        }
        let insert_at = self.next_atomic_boundary(self.cursor_pos);
        self.set_cursor(insert_at);
        let text = self.kill_buffer.clone();
        self.insert_str(&text);
    }

    pub(super) fn paste_line_after_current_line(&mut self) {
        let eol = self.end_of_current_line();
        let insert_at = if eol < self.text.len() { eol + 1 } else { eol };
        let cursor = if eol < self.text.len() {
            insert_at
        } else {
            insert_at + 1
        };
        let text = if eol < self.text.len() {
            if self.kill_buffer.ends_with('\n') {
                self.kill_buffer.clone()
            } else {
                format!("{}\n", self.kill_buffer)
            }
        } else {
            format!("\n{}", self.kill_buffer.trim_end_matches('\n'))
        };
        self.insert_str_at(insert_at, &text);
        self.set_cursor(cursor.min(self.text.len()));
    }

    pub(super) fn yank_current_line(&mut self) {
        let range = self.current_line_range_with_newline();
        self.yank_line_range(range);
    }

    pub(super) fn kill_current_line(&mut self) {
        let range = self.current_line_range_with_newline();
        self.kill_line_range(range);
    }

    pub(super) fn current_line_range_with_newline(&self) -> Range<usize> {
        let bol = self.beginning_of_current_line();
        let eol = self.end_of_current_line();
        let end = if eol < self.text.len() { eol + 1 } else { eol };
        bol..end
    }

    /// Move the cursor left by a single grapheme cluster.
    pub fn move_cursor_left(&mut self) {
        self.cursor_pos = self.prev_atomic_boundary(self.cursor_pos);
        self.preferred_col = None;
    }

    /// Move the cursor right by a single grapheme cluster.
    pub fn move_cursor_right(&mut self) {
        self.cursor_pos = self.next_atomic_boundary(self.cursor_pos);
        self.preferred_col = None;
    }

    pub fn move_cursor_up(&mut self) {
        // If we have a wrapping cache, prefer navigating across wrapped (visual) lines.
        if let Some((target_col, maybe_line)) = {
            let cache_ref = self.wrap_cache.borrow();
            if let Some(cache) = cache_ref.as_ref() {
                let lines = &cache.lines;
                if let Some((idx, col)) =
                    wrapping::cursor_position(&self.text, lines, cache.width, self.cursor_pos)
                {
                    let cur_range = &lines[idx];
                    // A saved column can outlive a resize. Do not land in hanging whitespace
                    // that is displayed on the following row.
                    let target_col = self
                        .preferred_col
                        .unwrap_or(col)
                        .min(usize::from(cache.width.saturating_sub(1)));
                    if idx > 0 {
                        let prev = &lines[idx - 1];
                        let line_start = prev.start;
                        let mut line_end = prev.end.saturating_sub(1);
                        if line_end == cur_range.start {
                            line_end = self.prev_atomic_boundary(line_end).max(line_start);
                        }
                        Some((target_col, Some((line_start, line_end))))
                    } else {
                        Some((target_col, None))
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } {
            // We had wrapping info. Apply movement accordingly.
            match maybe_line {
                Some((line_start, line_end)) => {
                    if self.preferred_col.is_none() {
                        self.preferred_col = Some(target_col);
                    }
                    self.move_to_display_col_on_line(line_start, line_end, target_col);
                    return;
                }
                None => {
                    // Already at first visual line -> move to start
                    self.cursor_pos = 0;
                    self.preferred_col = None;
                    return;
                }
            }
        }

        // Fallback to logical line navigation if we don't have wrapping info yet.
        if let Some(prev_nl) = self.text[..self.cursor_pos].rfind('\n') {
            let target_col = match self.preferred_col {
                Some(c) => c,
                None => {
                    let c = self.current_display_col();
                    self.preferred_col = Some(c);
                    c
                }
            };
            let prev_line_start = self.text[..prev_nl].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let prev_line_end = prev_nl;
            self.move_to_display_col_on_line(prev_line_start, prev_line_end, target_col);
        } else {
            self.cursor_pos = 0;
            self.preferred_col = None;
        }
    }

    pub fn move_cursor_down(&mut self) {
        // If we have a wrapping cache, prefer navigating across wrapped (visual) lines.
        if let Some((target_col, move_to_last)) = {
            let cache_ref = self.wrap_cache.borrow();
            if let Some(cache) = cache_ref.as_ref() {
                let lines = &cache.lines;
                if let Some((idx, col)) =
                    wrapping::cursor_position(&self.text, lines, cache.width, self.cursor_pos)
                {
                    let target_col = self
                        .preferred_col
                        .unwrap_or(col)
                        .min(usize::from(cache.width.saturating_sub(1)));
                    if idx + 1 < lines.len() {
                        let next = &lines[idx + 1];
                        let line_start = next.start;
                        let mut line_end = next.end.saturating_sub(1);
                        if lines
                            .get(idx + 2)
                            .is_some_and(|following| following.start == line_end)
                        {
                            line_end = self.prev_atomic_boundary(line_end).max(line_start);
                        }
                        Some((target_col, Some((line_start, line_end))))
                    } else {
                        Some((target_col, None))
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } {
            match move_to_last {
                Some((line_start, line_end)) => {
                    if self.preferred_col.is_none() {
                        self.preferred_col = Some(target_col);
                    }
                    self.move_to_display_col_on_line(line_start, line_end, target_col);
                    return;
                }
                None => {
                    // Already on last visual line -> move to end
                    self.cursor_pos = self.text.len();
                    self.preferred_col = None;
                    return;
                }
            }
        }

        // Fallback to logical line navigation if we don't have wrapping info yet.
        let target_col = match self.preferred_col {
            Some(c) => c,
            None => {
                let c = self.current_display_col();
                self.preferred_col = Some(c);
                c
            }
        };
        if let Some(next_nl) = self.text[self.cursor_pos..]
            .find('\n')
            .map(|i| i + self.cursor_pos)
        {
            let next_line_start = next_nl + 1;
            let next_line_end = self.text[next_line_start..]
                .find('\n')
                .map(|i| i + next_line_start)
                .unwrap_or(self.text.len());
            self.move_to_display_col_on_line(next_line_start, next_line_end, target_col);
        } else {
            self.cursor_pos = self.text.len();
            self.preferred_col = None;
        }
    }

    pub fn move_cursor_to_beginning_of_line(&mut self, move_up_at_bol: bool) {
        let bol = self.beginning_of_current_line();
        if move_up_at_bol && self.cursor_pos == bol {
            self.set_cursor(self.beginning_of_line(self.cursor_pos.saturating_sub(1)));
        } else {
            self.set_cursor(bol);
        }
        self.preferred_col = None;
    }

    pub fn move_cursor_to_end_of_line(&mut self, move_down_at_eol: bool) {
        let eol = self.end_of_current_line();
        if move_down_at_eol && self.cursor_pos == eol {
            let next_pos = (self.cursor_pos.saturating_add(1)).min(self.text.len());
            self.set_cursor(self.end_of_line(next_pos));
        } else {
            self.set_cursor(eol);
        }
    }
}
