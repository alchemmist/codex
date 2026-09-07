use super::*;

impl TextArea {
    pub(super) fn handle_vim_input(&mut self, event: KeyEvent) {
        if self.handle_vim_search_key(event) {
            return;
        }
        let prior_mode = self.vim_mode;
        if self.vim_visual.is_some() {
            self.handle_vim_visual(event);
        } else {
            match self.vim_mode {
                VimMode::Insert | VimMode::Replace => self.handle_vim_insert(event),
                VimMode::Normal => self.handle_vim_normal(event),
            }
        }
        if matches!(prior_mode, VimMode::Insert | VimMode::Replace)
            && self.vim_mode == VimMode::Normal
        {
            self.finish_pending_vim_change();
        }
    }

    pub(super) fn handle_vim_insert(&mut self, event: KeyEvent) {
        if matches!(event.code, KeyCode::Esc) {
            self.leave_vim_insert_mode();
            return;
        }
        if self.is_vim_replace_mode()
            && self.editor_keymap.delete_backward.is_pressed(event)
            && self.apply_vim_insert_action(VimAction::RestoreReplacedCharacter)
        {
            return;
        }
        let keymap = self.editor_keymap.clone();
        self.input_with_keymap(event, &keymap);
    }

    pub(super) fn leave_vim_insert_mode(&mut self) {
        let bol = self.beginning_of_current_line();
        if self.cursor_pos > bol {
            self.cursor_pos = self.prev_atomic_boundary(self.cursor_pos).max(bol);
        }
        self.enter_vim_normal_mode();
    }

    pub(super) fn handle_vim_normal(&mut self, event: KeyEvent) {
        let event = self.normalize_vim_command_event(event);
        let pending = std::mem::replace(&mut self.vim_pending, VimPending::None);
        match pending {
            VimPending::None => {}
            VimPending::Operator(op) => {
                self.handle_vim_operator(op, event);
                return;
            }
            VimPending::TextObject { operator, scope } => {
                self.handle_vim_text_object(operator, scope, event);
                return;
            }
            VimPending::Replace | VimPending::Find { .. } => {
                self.handle_vim_pending_command(pending, event);
                return;
            }
        }

        if visual_key(event, 'v') {
            self.start_vim_visual(VimVisualKind::Character);
            return;
        }
        if visual_shift_key(event, 'v') {
            self.start_vim_visual(VimVisualKind::Line);
            return;
        }
        if visual_control_key(event, 'v') {
            self.start_vim_visual(VimVisualKind::Block);
            return;
        }

        if self.vim_normal_keymap.enter_insert.is_pressed(event) {
            self.start_vim_edit(VimAction::Insert(VimInsertPosition::Cursor));
            return;
        }
        if self.vim_normal_keymap.append_after_cursor.is_pressed(event) {
            self.start_vim_edit(VimAction::Insert(VimInsertPosition::AfterCursor));
            return;
        }
        if self.vim_normal_keymap.append_line_end.is_pressed(event) {
            self.start_vim_edit(VimAction::Insert(VimInsertPosition::LineEnd));
            return;
        }
        if self.vim_normal_keymap.insert_line_start.is_pressed(event) {
            self.start_vim_edit(VimAction::Insert(VimInsertPosition::LineStart));
            return;
        }
        if self.vim_normal_keymap.open_line_below.is_pressed(event) {
            self.start_vim_edit(VimAction::Insert(VimInsertPosition::OpenBelow));
            return;
        }
        if self.vim_normal_keymap.open_line_above.is_pressed(event) {
            self.start_vim_edit(VimAction::Insert(VimInsertPosition::OpenAbove));
            return;
        }
        if self.vim_normal_keymap.move_left.is_pressed(event) {
            self.move_cursor_left();
            return;
        }
        if self.vim_normal_keymap.move_right.is_pressed(event) {
            self.move_cursor_right();
            return;
        }
        if self.vim_normal_keymap.move_down.is_pressed(event) {
            self.move_cursor_down();
            return;
        }
        if self.vim_normal_keymap.move_up.is_pressed(event) {
            self.move_cursor_up();
            return;
        }
        if self.vim_normal_keymap.move_word_forward.is_pressed(event) {
            self.set_cursor(self.beginning_of_next_word());
            return;
        }
        if self.vim_normal_keymap.move_word_backward.is_pressed(event) {
            self.set_cursor(self.beginning_of_previous_word());
            return;
        }
        if self.vim_normal_keymap.move_word_end.is_pressed(event) {
            self.set_cursor(self.vim_word_end_cursor());
            return;
        }
        if self.vim_normal_keymap.move_line_start.is_pressed(event) {
            self.set_cursor(self.beginning_of_current_line());
            return;
        }
        if self.vim_normal_keymap.move_line_end.is_pressed(event) {
            self.set_cursor(self.vim_line_end_cursor());
            return;
        }
        if self.vim_normal_keymap.delete_char.is_pressed(event) {
            self.start_vim_edit(VimAction::Delete(VimEditTarget::Character));
            return;
        }
        if self.vim_normal_keymap.substitute_char.is_pressed(event) {
            self.start_vim_edit(VimAction::Change(VimEditTarget::Character));
            return;
        }
        if self.vim_normal_keymap.delete_to_line_end.is_pressed(event) {
            self.start_vim_edit(VimAction::Delete(VimEditTarget::LineEnd));
            return;
        }
        if self.vim_normal_keymap.change_to_line_end.is_pressed(event) {
            self.start_vim_edit(VimAction::Change(VimEditTarget::LineEnd));
            return;
        }
        if self.vim_normal_keymap.yank_line.is_pressed(event) {
            self.yank_current_line();
            return;
        }
        if self.vim_normal_keymap.paste_after.is_pressed(event) {
            self.start_vim_edit(VimAction::PasteAfter);
            return;
        }
        if self
            .vim_normal_keymap
            .start_delete_operator
            .is_pressed(event)
        {
            self.vim_pending = VimPending::Operator(VimOperator::Delete);
            return;
        }
        if self.vim_normal_keymap.start_yank_operator.is_pressed(event) {
            self.vim_pending = VimPending::Operator(VimOperator::Yank);
            return;
        }
        if self
            .vim_normal_keymap
            .start_change_operator
            .is_pressed(event)
        {
            self.vim_pending = VimPending::Operator(VimOperator::Change);
            return;
        }
        if self.vim_normal_keymap.cancel_operator.is_pressed(event) {
            self.vim_pending = VimPending::None;
            self.cancel_vim_search();
            return;
        }
        self.handle_vim_extra_command(event);
    }

    pub(super) fn handle_vim_operator(&mut self, op: VimOperator, event: KeyEvent) -> bool {
        if op == VimOperator::Delete && self.vim_operator_keymap.delete_line.is_pressed(event) {
            self.start_vim_edit(VimAction::Delete(VimEditTarget::Line));
            return true;
        }
        if op == VimOperator::Yank && self.vim_operator_keymap.yank_line.is_pressed(event) {
            self.yank_current_line();
            return true;
        }
        if self.vim_operator_keymap.cancel.is_pressed(event) {
            return true;
        }
        if let Some(scope) = self.vim_text_object_scope_for_event(event) {
            self.vim_pending = VimPending::TextObject {
                operator: op,
                scope,
            };
            return true;
        }

        if let Some(motion) = self.vim_motion_for_event(event) {
            match op {
                VimOperator::Delete => {
                    self.start_vim_edit(VimAction::Delete(VimEditTarget::Motion(motion)));
                }
                VimOperator::Change => {
                    self.start_vim_edit(VimAction::Change(VimEditTarget::Motion(motion)));
                }
                VimOperator::Yank => self.apply_vim_operator(op, motion),
            }
            return true;
        }
        if op == VimOperator::Change
            && self
                .vim_normal_keymap
                .start_change_operator
                .is_pressed(event)
        {
            self.start_vim_edit(VimAction::Change(VimEditTarget::Line));
            return true;
        }
        self.handle_vim_operator_command(op, event)
    }

    pub(super) fn handle_vim_text_object(
        &mut self,
        op: VimOperator,
        scope: VimTextObjectScope,
        event: KeyEvent,
    ) -> bool {
        if self.vim_text_object_keymap.cancel.is_pressed(event) {
            return true;
        }
        let Some(object) = self.vim_text_object_for_event(event) else {
            return false;
        };
        match op {
            VimOperator::Delete => {
                self.start_vim_edit(VimAction::Delete(VimEditTarget::TextObject {
                    scope,
                    object,
                }));
            }
            VimOperator::Change => {
                self.start_vim_edit(VimAction::Change(VimEditTarget::TextObject {
                    scope,
                    object,
                }));
            }
            VimOperator::Yank => {
                if let Some(range) = self.text_object_range(object, scope) {
                    self.apply_vim_operator_to_range(op, range);
                }
            }
        }
        true
    }

    pub(super) fn vim_motion_for_event(&self, event: KeyEvent) -> Option<VimMotion> {
        if self.vim_operator_keymap.motion_left.is_pressed(event) {
            return Some(VimMotion::Left);
        }
        if self.vim_operator_keymap.motion_right.is_pressed(event) {
            return Some(VimMotion::Right);
        }
        if self.vim_operator_keymap.motion_down.is_pressed(event) {
            return Some(VimMotion::Down);
        }
        if self.vim_operator_keymap.motion_up.is_pressed(event) {
            return Some(VimMotion::Up);
        }
        if self
            .vim_operator_keymap
            .motion_word_forward
            .is_pressed(event)
        {
            return Some(VimMotion::WordForward);
        }
        if self
            .vim_operator_keymap
            .motion_word_backward
            .is_pressed(event)
        {
            return Some(VimMotion::WordBackward);
        }
        if self.vim_operator_keymap.motion_word_end.is_pressed(event) {
            return Some(VimMotion::WordEnd);
        }
        if self.vim_operator_keymap.motion_line_start.is_pressed(event) {
            return Some(VimMotion::LineStart);
        }
        if self.vim_operator_keymap.motion_line_end.is_pressed(event) {
            return Some(VimMotion::LineEnd);
        }
        None
    }

    pub(super) fn apply_vim_operator(&mut self, op: VimOperator, motion: VimMotion) {
        if op == VimOperator::Change && motion == VimMotion::WordForward {
            let target = if self.text[self.cursor_pos..]
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_whitespace())
            {
                self.end_of_next_word()
            } else {
                self.beginning_of_next_word()
                    .min(self.end_of_current_line())
            };
            if target > self.cursor_pos {
                self.apply_vim_operator_to_range(op, self.cursor_pos..target);
            } else {
                self.vim_mode = VimMode::Insert;
            }
            return;
        }
        let Some(range) = self.range_for_motion(motion) else {
            if op == VimOperator::Change && motion == VimMotion::LineEnd {
                self.vim_mode = VimMode::Insert;
            }
            return;
        };
        if op == VimOperator::Change && matches!(motion, VimMotion::Up | VimMotion::Down) {
            if motion == VimMotion::Up && self.beginning_of_current_line() == 0
                || motion == VimMotion::Down && self.end_of_current_line() == self.text.len()
            {
                return;
            }
            let retain_newline =
                range.end < self.text.len() && self.text[range.clone()].ends_with('\n');
            let start = range.start;
            self.kill_line_range(range);
            if retain_newline {
                self.insert_str_at(start, "\n");
                self.set_cursor(start);
            }
            self.vim_mode = VimMode::Insert;
            return;
        }
        self.apply_vim_operator_to_range(op, range);
    }

    pub(super) fn apply_vim_operator_to_range(&mut self, op: VimOperator, range: Range<usize>) {
        match op {
            VimOperator::Delete => self.kill_range(range),
            VimOperator::Yank => self.yank_range(range),
            VimOperator::Change => {
                self.kill_range(range);
                self.vim_mode = VimMode::Insert;
            }
        }
    }

    pub(super) fn range_for_motion(&mut self, motion: VimMotion) -> Option<Range<usize>> {
        if matches!(motion, VimMotion::Up | VimMotion::Down) {
            return self.linewise_range_for_vertical_motion(motion);
        }
        let start = self.cursor_pos;
        let target = self.target_for_motion(motion);
        if start == target {
            return None;
        }
        let (range_start, range_end) = if target < start {
            (target, start)
        } else {
            (start, target)
        };
        Some(range_start..range_end)
    }

    pub(super) fn linewise_range_for_vertical_motion(
        &self,
        motion: VimMotion,
    ) -> Option<Range<usize>> {
        let current = self.current_line_range_with_newline();
        let range = match motion {
            VimMotion::Up => {
                let start = if current.start == 0 {
                    current.start
                } else {
                    self.beginning_of_line(current.start.saturating_sub(1))
                };
                start..current.end
            }
            VimMotion::Down => {
                let end = if current.end >= self.text.len() {
                    current.end
                } else {
                    let next_eol = self.end_of_line(current.end);
                    if next_eol < self.text.len() {
                        next_eol + 1
                    } else {
                        next_eol
                    }
                };
                current.start..end
            }
            VimMotion::Left
            | VimMotion::Right
            | VimMotion::WordForward
            | VimMotion::WordBackward
            | VimMotion::WordEnd
            | VimMotion::LineStart
            | VimMotion::LineEnd => return None,
        };
        (range.start < range.end).then_some(range)
    }

    pub(super) fn target_for_motion(&mut self, motion: VimMotion) -> usize {
        let original_cursor = self.cursor_pos;
        let original_preferred = self.preferred_col;
        match motion {
            VimMotion::Left => self.move_cursor_left(),
            VimMotion::Right => self.move_cursor_right(),
            VimMotion::Up => self.move_cursor_up(),
            VimMotion::Down => self.move_cursor_down(),
            VimMotion::WordForward => self.set_cursor(self.beginning_of_next_word()),
            VimMotion::WordBackward => self.set_cursor(self.beginning_of_previous_word()),
            VimMotion::WordEnd => self.set_cursor(self.vim_word_end_exclusive()),
            VimMotion::LineStart => self.set_cursor(self.beginning_of_current_line()),
            VimMotion::LineEnd => self.set_cursor(self.end_of_current_line()),
        }
        let target = self.cursor_pos;
        self.cursor_pos = original_cursor;
        self.preferred_col = original_preferred;
        target
    }
}
