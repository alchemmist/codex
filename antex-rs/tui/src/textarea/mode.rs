use super::*;

impl TextArea {
    /// Enable or disable modal Vim editing for the textarea.
    ///
    /// Enabling always enters normal mode and disabling always returns to
    /// insert semantics. Pending operators are cleared in both directions so a
    /// toggle cannot leave the next keypress interpreted as the second half of
    /// an old `d` or `y` command.
    pub(crate) fn set_vim_enabled(&mut self, enabled: bool) {
        self.vim_enabled = enabled;
        self.vim_pending = VimPending::None;
        self.vim_search = vim_search::VimSearch::default();
        self.vim_commands = VimCommandState::default();
        self.vim_mode = if enabled {
            self.configured_vim_mode_start()
        } else {
            VimMode::Insert
        };
    }

    /// Return whether modal Vim editing is currently enabled.
    pub(crate) fn is_vim_enabled(&self) -> bool {
        self.vim_enabled
    }

    pub(crate) fn set_vim_mode_indicator_enabled(&mut self, enabled: bool) {
        self.vim_mode_indicator_enabled = enabled;
    }

    pub(crate) fn set_vim_mode_start(&mut self, start: VimModeStart) {
        self.vim_mode_start = start;
    }

    pub(super) fn configured_vim_mode_start(&self) -> VimMode {
        match self.vim_mode_start {
            VimModeStart::Normal => VimMode::Normal,
            VimModeStart::Insert => VimMode::Insert,
        }
    }

    /// Return whether Vim mode is enabled and currently waiting in normal mode.
    ///
    /// Composer-level handlers use this to decide whether Up/Down should be
    /// offered to history navigation only after normal-mode movement reaches a
    /// text boundary.
    pub(crate) fn is_vim_normal_mode(&self) -> bool {
        self.vim_enabled && self.vim_mode == VimMode::Normal && self.vim_visual.is_none()
    }

    pub(crate) fn take_system_clipboard_yank(&mut self) -> Option<String> {
        self.pending_system_clipboard_yank.take()
    }

    /// Return the cursor position that represents the last editable item in Vim normal mode.
    /// Return whether a Vim operator is waiting for a motion.
    ///
    /// This is observable so the composer can avoid stealing the second key of
    /// `d{motion}` or `y{motion}` for higher-level shortcuts.
    pub(crate) fn is_vim_operator_pending(&self) -> bool {
        self.vim_query().is_some() || !matches!(self.vim_pending, VimPending::None)
    }

    /// Return the keymap context that owns the next editing key.
    pub(crate) fn keymap_context(&self) -> KeymapContext {
        if !self.vim_enabled
            || matches!(self.vim_mode, VimMode::Insert | VimMode::Replace)
            || self.vim_query().is_some()
        {
            return KeymapContext::Editor;
        }
        match self.vim_pending {
            VimPending::None => KeymapContext::VimNormal,
            VimPending::Replace | VimPending::Find { .. } => KeymapContext::Editor,
            VimPending::Operator(_) => KeymapContext::VimOperator,
            VimPending::TextObject { .. } => KeymapContext::VimTextObject,
        }
    }

    /// Enter Vim insert mode if modal editing is enabled.
    ///
    /// Calling this while Vim is disabled is a no-op, which lets parent
    /// workflows reset mode after submissions without first branching on the
    /// current keymap state.
    pub(crate) fn enter_vim_insert_mode(&mut self) {
        if self.vim_enabled {
            self.vim_mode = VimMode::Insert;
            self.vim_visual = None;
            self.vim_pending = VimPending::None;
            self.clear_vim_replace_recovery();
            self.cancel_vim_search();
            if self.vim_commands.pending_change.is_empty() && !self.vim_commands.replaying {
                self.start_vim_edit(VimAction::Insert(VimInsertPosition::Cursor));
            }
        }
    }

    /// Enter Vim normal mode if modal editing is enabled.
    ///
    /// This clears any pending operator and preferred vertical column. The
    /// latter matches normal Vim navigation expectations after leaving insert
    /// mode; preserving the old column would make the next `j` or `k` jump to a
    /// stale visual target.
    pub(crate) fn enter_vim_normal_mode(&mut self) {
        if self.vim_enabled {
            self.vim_mode = VimMode::Normal;
            self.vim_visual = None;
            self.vim_pending = VimPending::None;
            self.cancel_vim_search();
            self.preferred_col = None;
            self.clear_vim_replace_recovery();
        }
    }

    /// Return whether rapid plain-key bursts should be treated as paste input.
    ///
    /// Paste burst detection is disabled in Vim normal mode so a fast sequence
    /// like `dd` or `yw` remains command input instead of being converted into
    /// literal text.
    pub(crate) fn allows_paste_burst(&self) -> bool {
        !self.vim_enabled || matches!(self.vim_mode, VimMode::Insert | VimMode::Replace)
    }

    /// Return whether rendering should use the insert-mode cursor style.
    /// Return whether Escape should be intercepted before composer-level routing.
    ///
    /// In Vim insert mode or while a command is pending, Escape is an editing
    /// transition rather than a popup cancel/backtrack or turn-interrupt shortcut.
    pub(crate) fn should_handle_vim_insert_escape(&self, event: KeyEvent) -> bool {
        self.vim_enabled
            && (matches!(self.vim_mode, VimMode::Insert | VimMode::Replace)
                || self.vim_visual.is_some()
                || !matches!(self.vim_pending, VimPending::None))
            && event.code == KeyCode::Esc
            && event.modifiers == KeyModifiers::NONE
            && matches!(event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
    }

    pub(crate) fn normalize_vim_command_event(&self, event: KeyEvent) -> KeyEvent {
        if self.vim_enabled
            && self.vim_mode == VimMode::Normal
            && !matches!(
                self.vim_pending,
                VimPending::Replace | VimPending::Find { .. }
            )
        {
            russian_layout::remap_vim_command_event(event)
        } else {
            event
        }
    }

    /// Return the footer label for the active Vim mode.
    ///
    /// `None` means Vim editing is disabled, so callers should omit the mode
    /// indicator rather than rendering an insert-mode label for normal
    /// non-modal editing.
    pub(crate) fn vim_mode_label(&self) -> Option<&'static str> {
        if !self.vim_enabled {
            return None;
        }
        Some(match self.vim_mode {
            VimMode::Normal => self.vim_visual_label().unwrap_or("Normal"),
            VimMode::Insert => "Insert",
            VimMode::Replace => "Replace",
        })
    }

    /// Return the styled footer indicator for the active Vim editing mode.
    pub(crate) fn vim_mode_indicator_span(&self) -> Option<Span<'static>> {
        if !self.vim_enabled || !self.vim_mode_indicator_enabled {
            return None;
        }
        Some(match self.vim_mode {
            VimMode::Normal => match self.vim_visual_label() {
                Some(label) => format!("Vim: {label}").cyan(),
                None => "Vim: Normal".magenta(),
            },
            VimMode::Insert => "Vim: Insert".green(),
            VimMode::Replace => "Vim: Replace".cyan(),
        })
    }
}
