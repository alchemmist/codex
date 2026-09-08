use std::collections::HashSet;
use std::collections::VecDeque;

use super::*;

#[derive(Clone)]
struct Snapshot {
    draft: Draft,
    cursor: usize,
}

#[derive(Default)]
pub(super) struct EditHistory {
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
    pending: Option<Snapshot>,
}

impl EditHistory {
    fn trim(&mut self) {
        loop {
            let mut images = HashSet::new();
            let mut text_bytes = 0;
            let mut image_bytes = 0;
            for snapshot in self.undo.iter().chain(&self.redo) {
                text_bytes += snapshot.draft.text.len()
                    + snapshot.draft.elements.len() * std::mem::size_of::<TextElement>();
                for (_, image) in &snapshot.draft.images {
                    if let Content::Image { data, .. } = image
                        && images.insert(data.as_ptr() as usize)
                    {
                        image_bytes += data.len();
                    }
                }
            }
            if self.undo.len() + self.redo.len() <= 64
                && text_bytes <= 1024 * 1024
                && image_bytes <= 16 * 1024 * 1024
            {
                break;
            }
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }
}

impl Composer {
    pub(super) fn restore_draft(&mut self, draft: Draft, cursor: usize) {
        self.editor
            .set_text_with_elements(&draft.text, &draft.elements);
        self.editor.set_cursor(cursor);
        self.images = draft.images;
        self.state = TextAreaState::default();
    }

    pub(super) fn history_key(&mut self, event: KeyEvent) -> bool {
        if !self.editor.is_vim_normal_mode() || self.editor.is_vim_operator_pending() {
            return false;
        }
        let redo = self.keymap.vim_normal.redo.is_pressed(event);
        let undo = self.keymap.vim_normal.undo.is_pressed(event);
        if !redo && !undo {
            return false;
        }
        let snapshot = if redo {
            self.history.redo.pop_back()
        } else {
            self.history.undo.pop_back()
        };
        if let Some(snapshot) = snapshot {
            let current = Snapshot {
                draft: self.draft(),
                cursor: self.editor.cursor(),
            };
            if redo {
                self.history.undo.push_back(current);
            } else {
                self.history.redo.push_back(current);
            }
            let mut persistent = crate::textarea::VimPersistentState::default();
            self.editor.swap_vim_persistent_state(&mut persistent);
            self.restore_draft(snapshot.draft, snapshot.cursor);
            self.editor.swap_vim_persistent_state(&mut persistent);
            self.editor.enter_vim_normal_mode();
            self.history.trim();
        }
        true
    }

    pub(super) fn begin_edit(&mut self, event: KeyEvent) {
        if !self.editor.is_vim_enabled()
            || self.history.pending.is_some()
            || self.editor.is_vim_operator_pending()
        {
            return;
        }
        let map = &self.keymap.vim_normal;
        if self.editor.is_vim_normal_mode()
            && ([
                &map.move_left,
                &map.move_right,
                &map.move_up,
                &map.move_down,
                &map.move_word_forward,
                &map.move_word_backward,
                &map.move_word_end,
                &map.move_line_start,
                &map.move_line_end,
                &map.find_forward,
                &map.find_backward,
                &map.jump_top,
                &map.jump_bottom,
                &map.yank_line,
                &map.start_yank_operator,
                &map.cancel_operator,
            ]
            .iter()
            .any(|bindings| bindings.is_pressed(event))
                || self.editor.wants_vim_search_key(event))
        {
            return;
        }
        self.capture_edit();
    }

    pub(super) fn begin_direct_edit(&mut self) {
        if self.capture_edit() {
            let mut persistent = crate::textarea::VimPersistentState::default();
            self.editor.swap_vim_persistent_state(&mut persistent);
            persistent.commands.last_change.clear();
            self.editor.swap_vim_persistent_state(&mut persistent);
        }
    }

    fn capture_edit(&mut self) -> bool {
        if self.editor.is_vim_enabled()
            && self.history.pending.is_none()
            && !self.editor.is_vim_operator_pending()
        {
            self.history.pending = Some(Snapshot {
                draft: self.draft(),
                cursor: self.editor.cursor(),
            });
            return true;
        }
        false
    }

    pub(super) fn finish_edit(&mut self) {
        if !self.editor.is_vim_normal_mode() || self.editor.is_vim_operator_pending() {
            return;
        }
        if let Some(snapshot) = self.history.pending.take()
            && snapshot.draft != self.draft()
        {
            self.history.redo.clear();
            self.history.undo.push_back(snapshot);
            self.history.trim();
        }
    }
}
