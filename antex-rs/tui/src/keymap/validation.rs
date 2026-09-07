use super::*;

impl RuntimeKeymap {
    /// Reject ambiguous bindings in scopes that are evaluated together.
    ///
    /// App actions are checked before composer actions. Contexts with hard-coded
    /// sequence behavior stay outside this configurable keymap.
    pub(super) fn validate_conflicts(&self) -> Result<(), String> {
        for (action, bindings) in [
            (
                "previous_permission_mode",
                &self.chat.previous_permission_mode,
            ),
            ("next_permission_mode", &self.chat.next_permission_mode),
            ("prompt_stack_back", &self.chat.prompt_stack_back),
            ("skip_question", &self.chat.skip_question),
        ] {
            if bindings.iter().any(|binding| {
                let (code, modifiers) = binding.parts();
                crate::key_hint::is_plain_text_key_event(KeyEvent::new(code, modifiers))
                    || matches!(code, KeyCode::Char(_)) && crate::key_hint::is_altgr(modifiers)
            }) {
                return Err(format!(
                    "tui.keymap.chat.{action}: printable keys are reserved for text input"
                ));
            }
        }
        #[cfg(unix)]
        if self
            .app
            .open_agents
            .contains(&key_hint::ctrl(KeyCode::Char('z')))
        {
            return Err(
                "tui.keymap.global.open_agents: ctrl-z is reserved for suspend".to_string(),
            );
        }
        if self.app.open_agents.iter().any(|binding| {
            matches!(binding.parts(), (KeyCode::Char(_), modifiers)
                if crate::key_hint::is_altgr(modifiers))
        }) {
            return Err(
                "tui.keymap.global.open_agents: AltGr characters are reserved for text input"
                    .to_string(),
            );
        }
        let mut side_toggle_bindings = self.app.toggle_side_conversation.clone();
        let slash_binding = key_hint::ctrl(KeyCode::Char('/'));
        let legacy_slash_binding = key_hint::ctrl(KeyCode::Char('7'));
        if side_toggle_bindings.contains(&slash_binding)
            && !side_toggle_bindings.contains(&legacy_slash_binding)
        {
            side_toggle_bindings.push(legacy_slash_binding);
        }

        let main_bindings = [
            ("open_agents", self.app.open_agents.as_slice()),
            ("open_transcript", self.app.open_transcript.as_slice()),
            (
                "open_external_editor",
                self.app.open_external_editor.as_slice(),
            ),
            ("copy", self.app.copy.as_slice()),
            ("clear_terminal", self.app.clear_terminal.as_slice()),
            ("toggle_vim_mode", self.app.toggle_vim_mode.as_slice()),
            ("toggle_fast_mode", self.app.toggle_fast_mode.as_slice()),
            ("toggle_raw_output", self.app.toggle_raw_output.as_slice()),
            ("toggle_side_conversation", side_toggle_bindings.as_slice()),
            ("chat.interrupt_turn", self.chat.interrupt_turn.as_slice()),
            (
                "chat.decrease_reasoning_effort",
                self.chat.decrease_reasoning_effort.as_slice(),
            ),
            (
                "chat.increase_reasoning_effort",
                self.chat.increase_reasoning_effort.as_slice(),
            ),
            (
                "chat.previous_permission_mode",
                self.chat.previous_permission_mode.as_slice(),
            ),
            (
                "chat.next_permission_mode",
                self.chat.next_permission_mode.as_slice(),
            ),
            (
                "chat.edit_queued_message",
                self.chat.edit_queued_message.as_slice(),
            ),
            (
                "chat.prompt_stack_back",
                self.chat.prompt_stack_back.as_slice(),
            ),
            ("chat.skip_question", self.chat.skip_question.as_slice()),
            ("composer.submit", self.composer.submit.as_slice()),
            ("composer.queue", self.composer.queue.as_slice()),
            (
                "composer.toggle_shortcuts",
                self.composer.toggle_shortcuts.as_slice(),
            ),
            (
                "composer.history_search_previous",
                self.composer.history_search_previous.as_slice(),
            ),
            (
                "composer.history_search_next",
                self.composer.history_search_next.as_slice(),
            ),
        ];
        validate_unique("app", main_bindings)?;

        validate_no_reserved(
            "main",
            main_bindings,
            MAIN_RESERVED_BINDINGS,
            [(
                "chat.interrupt_turn",
                "fixed.backtrack",
                key_hint::plain(KeyCode::Esc),
            )],
        )?;

        validate_no_reserved(
            "main",
            main_bindings,
            &[("fixed.prompt_stash", key_hint::ctrl(KeyCode::Char('s')))],
            [(
                "composer.history_search_next",
                "fixed.prompt_stash",
                key_hint::ctrl(KeyCode::Char('s')),
            )],
        )?;

        let approval_overlay_bindings = [
            ("list.move_up", self.list.move_up.as_slice()),
            ("list.move_down", self.list.move_down.as_slice()),
            ("list.move_left", self.list.move_left.as_slice()),
            ("list.move_right", self.list.move_right.as_slice()),
            ("list.page_up", self.list.page_up.as_slice()),
            ("list.page_down", self.list.page_down.as_slice()),
            ("list.jump_top", self.list.jump_top.as_slice()),
            ("list.jump_bottom", self.list.jump_bottom.as_slice()),
            ("list.accept", self.list.accept.as_slice()),
            ("list.cancel", self.list.cancel.as_slice()),
            (
                "approval.open_fullscreen",
                self.approval.open_fullscreen.as_slice(),
            ),
            ("approval.open_thread", self.approval.open_thread.as_slice()),
            ("approval.approve", self.approval.approve.as_slice()),
            (
                "approval.approve_for_session",
                self.approval.approve_for_session.as_slice(),
            ),
            (
                "approval.approve_for_prefix",
                self.approval.approve_for_prefix.as_slice(),
            ),
            ("approval.deny", self.approval.deny.as_slice()),
            ("approval.decline", self.approval.decline.as_slice()),
            ("approval.cancel", self.approval.cancel.as_slice()),
        ];
        validate_no_shadow_with_allowed_overlaps(
            "app",
            [
                ("open_agents", self.app.open_agents.as_slice()),
                ("open_transcript", self.app.open_transcript.as_slice()),
                (
                    "open_external_editor",
                    self.app.open_external_editor.as_slice(),
                ),
                ("copy", self.app.copy.as_slice()),
                ("clear_terminal", self.app.clear_terminal.as_slice()),
                ("toggle_vim_mode", self.app.toggle_vim_mode.as_slice()),
                ("toggle_fast_mode", self.app.toggle_fast_mode.as_slice()),
                ("toggle_raw_output", self.app.toggle_raw_output.as_slice()),
                ("toggle_side_conversation", side_toggle_bindings.as_slice()),
            ],
            approval_overlay_bindings,
            [(
                "clear_terminal",
                "list.move_right",
                key_hint::ctrl(KeyCode::Char('l')),
            )],
        )?;

        // The request-user-input overlay consumes turn interruption before
        // configurable question navigation reaches its list handler.
        validate_no_shadow_with_allowed_overlaps(
            "request_user_input",
            [("chat.interrupt_turn", self.chat.interrupt_turn.as_slice())],
            [
                ("list.move_left", self.list.move_left.as_slice()),
                ("list.move_right", self.list.move_right.as_slice()),
            ],
            [],
        )?;

        // Async question shortcuts run before option selection and text editing.
        validate_no_shadow_with_allowed_overlaps(
            "async_questions",
            [
                (
                    "chat.prompt_stack_back",
                    self.chat.prompt_stack_back.as_slice(),
                ),
                ("chat.skip_question", self.chat.skip_question.as_slice()),
            ],
            [
                ("list.move_up", self.list.move_up.as_slice()),
                ("list.move_down", self.list.move_down.as_slice()),
                ("list.accept", self.list.accept.as_slice()),
            ],
            [],
        )?;

        // While the composer is focused, these main-surface handlers always
        // consume matching keys before the event reaches the textarea editor.
        validate_no_shadow_with_allowed_overlaps(
            "main",
            [
                ("open_agents", self.app.open_agents.as_slice()),
                ("open_transcript", self.app.open_transcript.as_slice()),
                (
                    "open_external_editor",
                    self.app.open_external_editor.as_slice(),
                ),
                ("copy", self.app.copy.as_slice()),
                ("clear_terminal", self.app.clear_terminal.as_slice()),
                ("chat.interrupt_turn", self.chat.interrupt_turn.as_slice()),
                (
                    "chat.decrease_reasoning_effort",
                    self.chat.decrease_reasoning_effort.as_slice(),
                ),
                (
                    "chat.increase_reasoning_effort",
                    self.chat.increase_reasoning_effort.as_slice(),
                ),
                (
                    "chat.previous_permission_mode",
                    self.chat.previous_permission_mode.as_slice(),
                ),
                (
                    "chat.next_permission_mode",
                    self.chat.next_permission_mode.as_slice(),
                ),
                (
                    "chat.prompt_stack_back",
                    self.chat.prompt_stack_back.as_slice(),
                ),
                ("chat.skip_question", self.chat.skip_question.as_slice()),
                ("composer.submit", self.composer.submit.as_slice()),
                ("toggle_vim_mode", self.app.toggle_vim_mode.as_slice()),
                ("toggle_fast_mode", self.app.toggle_fast_mode.as_slice()),
                ("toggle_raw_output", self.app.toggle_raw_output.as_slice()),
                ("toggle_side_conversation", side_toggle_bindings.as_slice()),
                (
                    "composer.history_search_previous",
                    self.composer.history_search_previous.as_slice(),
                ),
            ],
            [
                (
                    "editor.insert_newline",
                    self.editor.insert_newline.as_slice(),
                ),
                ("editor.move_left", self.editor.move_left.as_slice()),
                ("editor.move_right", self.editor.move_right.as_slice()),
                ("editor.move_up", self.editor.move_up.as_slice()),
                ("editor.move_down", self.editor.move_down.as_slice()),
                (
                    "editor.move_word_left",
                    self.editor.move_word_left.as_slice(),
                ),
                (
                    "editor.move_word_right",
                    self.editor.move_word_right.as_slice(),
                ),
                (
                    "editor.move_line_start",
                    self.editor.move_line_start.as_slice(),
                ),
                ("editor.move_line_end", self.editor.move_line_end.as_slice()),
                (
                    "editor.delete_backward",
                    self.editor.delete_backward.as_slice(),
                ),
                (
                    "editor.delete_forward",
                    self.editor.delete_forward.as_slice(),
                ),
                (
                    "editor.delete_backward_word",
                    self.editor.delete_backward_word.as_slice(),
                ),
                (
                    "editor.delete_forward_word",
                    self.editor.delete_forward_word.as_slice(),
                ),
                (
                    "editor.kill_line_start",
                    self.editor.kill_line_start.as_slice(),
                ),
                (
                    "editor.kill_whole_line",
                    self.editor.kill_whole_line.as_slice(),
                ),
                ("editor.kill_line_end", self.editor.kill_line_end.as_slice()),
                ("editor.yank", self.editor.yank.as_slice()),
            ],
            [(
                "composer.submit",
                "editor.insert_newline",
                key_hint::plain(KeyCode::Enter),
            )],
        )?;

        let context_bindings = |context| {
            runtime_action_bindings(self)
                .filter(move |binding| binding.id.context == context)
                .map(|binding| (binding.id.action, binding.bindings))
        };
        for context in [
            KeymapContext::Editor,
            KeymapContext::VimNormal,
            KeymapContext::VimOperator,
            KeymapContext::VimTextObject,
            KeymapContext::Pager,
        ] {
            validate_unique(context.config_name(), context_bindings(context))?;
        }

        validate_no_reserved(
            "pager",
            context_bindings(KeymapContext::Pager),
            TRANSCRIPT_BACKTRACK_RESERVED_BINDINGS,
            [],
        )?;

        validate_unique("list", context_bindings(KeymapContext::List))?;

        validate_unique("agents", context_bindings(KeymapContext::Agents))?;
        validate_no_reserved(
            "agents",
            context_bindings(KeymapContext::Agents),
            MAIN_RESERVED_BINDINGS,
            [],
        )?;
        for (action, bindings) in context_bindings(KeymapContext::Agents) {
            #[cfg(unix)]
            if bindings.contains(&key_hint::ctrl(KeyCode::Char('z'))) {
                return Err(format!(
                    "tui.keymap.agents.{action}: ctrl-z is reserved for suspend"
                ));
            }
            if bindings.iter().any(|binding| {
                matches!(binding.parts(), (KeyCode::Char(_), modifiers)
                    if modifiers.is_empty()
                        || modifiers == KeyModifiers::SHIFT
                        || crate::key_hint::is_altgr(modifiers))
                    || binding.parts() == (KeyCode::Backspace, KeyModifiers::NONE)
            }) {
                return Err(format!(
                    "tui.keymap.agents.{action}: printable keys and backspace are reserved for task input"
                ));
            }
        }

        validate_unique("approval", context_bindings(KeymapContext::Approval))?;

        let mut seen: HashMap<(KeyCode, KeyModifiers), &'static str> = HashMap::new();
        for (action, bindings) in approval_overlay_bindings {
            for binding in bindings {
                let key = binding.normalized_parts();
                if let Some(previous) = seen.insert(key, action) {
                    // Approval overlays intentionally reserve Esc as a stable
                    // cancellation path even though decline options may also
                    // display it in contexts where that is safe.
                    if previous == "list.cancel"
                        && action == "approval.decline"
                        && key == (KeyCode::Esc, KeyModifiers::NONE)
                    {
                        continue;
                    }
                    return Err(format!(
                        "Ambiguous approval overlay keymap bindings: `{previous}` and `{action}` use the same key. \
Set unique keys in `~/.codex/config.toml` and retry. \
See the Codex keymap documentation for supported actions and examples."
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Reject duplicate keys inside one effective context map.
///
/// This intentionally allows the same key across different contexts; handlers
/// only evaluate one context at a time.
pub(super) fn validate_unique<'a>(
    context: &str,
    pairs: impl IntoIterator<Item = (&'static str, &'a [KeyBinding])>,
) -> Result<(), String> {
    let mut seen: HashMap<(KeyCode, KeyModifiers), &'static str> = HashMap::new();
    for (action, bindings) in pairs {
        for binding in bindings {
            let key = binding.normalized_parts();
            if let Some(previous) = seen.insert(key, action) {
                return Err(format!(
                    "Ambiguous `tui.keymap.{context}` bindings: `{previous}` and `{action}` use the same key. \
Set unique keys in `~/.codex/config.toml` and retry. \
See the Codex keymap documentation for supported actions and examples."
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_no_shadow_with_allowed_overlaps<
    const N: usize,
    const M: usize,
    const A: usize,
>(
    context: &str,
    primary: [(&'static str, &[KeyBinding]); N],
    shadowed: [(&'static str, &[KeyBinding]); M],
    allowed_overlaps: [(&'static str, &'static str, KeyBinding); A],
) -> Result<(), String> {
    let mut seen: HashMap<(KeyCode, KeyModifiers), &'static str> = HashMap::new();
    for (action, bindings) in primary {
        for binding in bindings {
            seen.insert(binding.normalized_parts(), action);
        }
    }
    for (action, bindings) in shadowed {
        for binding in bindings {
            let key = binding.normalized_parts();
            if let Some(previous) = seen.get(&key) {
                if allowed_overlaps.iter().any(
                    |(allowed_primary, allowed_shadowed, allowed_binding)| {
                        *allowed_primary == *previous
                            && *allowed_shadowed == action
                            && allowed_binding.normalized_parts() == key
                    },
                ) {
                    continue;
                }
                return Err(format!(
                    "Ambiguous `tui.keymap.{context}` bindings: `{previous}` shadows `{action}` with the same key. \
Set unique keys in `~/.codex/config.toml` and retry. \
See the Codex keymap documentation for supported actions and examples."
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_no_reserved<'a, const A: usize>(
    context: &str,
    pairs: impl IntoIterator<Item = (&'static str, &'a [KeyBinding])>,
    reserved: &[(&'static str, KeyBinding)],
    allowed_overlaps: [(&'static str, &'static str, KeyBinding); A],
) -> Result<(), String> {
    for (action, bindings) in pairs {
        for binding in bindings {
            let key = binding.parts();
            if let Some((reserved_action, _)) = reserved
                .iter()
                .find(|(_, reserved_binding)| reserved_binding.parts() == key)
            {
                if allowed_overlaps.iter().any(
                    |(allowed_action, allowed_reserved_action, allowed_binding)| {
                        *allowed_action == action
                            && *allowed_reserved_action == *reserved_action
                            && allowed_binding.parts() == key
                    },
                ) {
                    continue;
                }
                return Err(format!(
                    "Ambiguous `tui.keymap.{context}` bindings: `{action}` uses a key reserved by `{reserved_action}`. \
Set a different key in `~/.codex/config.toml` and retry. \
See the Codex keymap documentation for supported actions and examples."
                ));
            }
        }
    }
    Ok(())
}

pub(super) const MAIN_RESERVED_BINDINGS: &[(&str, KeyBinding)] = &[
    (
        "fixed.interrupt_or_quit",
        key_hint::ctrl(KeyCode::Char('c')),
    ),
    ("fixed.quit", key_hint::ctrl(KeyCode::Char('d'))),
    ("fixed.paste_image", key_hint::ctrl(KeyCode::Char('v'))),
    ("fixed.paste_image", key_hint::ctrl_alt(KeyCode::Char('v'))),
    (
        "fixed.cycle_collaboration_mode",
        key_hint::shift(KeyCode::Tab),
    ),
    ("fixed.backtrack", key_hint::plain(KeyCode::Esc)),
    ("fixed.previous_agent", key_hint::alt(KeyCode::Left)),
    ("fixed.next_agent", key_hint::alt(KeyCode::Right)),
    ("fixed.slash_command", key_hint::plain(KeyCode::Char('/'))),
    ("fixed.shell_command", key_hint::plain(KeyCode::Char('!'))),
    ("fixed.file_paths", key_hint::plain(KeyCode::Char('@'))),
    (
        "fixed.connector_mentions",
        key_hint::plain(KeyCode::Char('$')),
    ),
];

pub(super) const TRANSCRIPT_BACKTRACK_RESERVED_BINDINGS: &[(&str, KeyBinding)] = &[
    (
        "fixed.transcript_edit_previous",
        key_hint::plain(KeyCode::Esc),
    ),
    (
        "fixed.transcript_edit_previous",
        key_hint::plain(KeyCode::Left),
    ),
    (
        "fixed.transcript_edit_next",
        key_hint::plain(KeyCode::Right),
    ),
    (
        "fixed.transcript_confirm_edit",
        key_hint::plain(KeyCode::Enter),
    ),
];
