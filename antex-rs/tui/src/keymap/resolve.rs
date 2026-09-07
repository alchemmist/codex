use super::*;

impl RuntimeKeymap {
    /// Return built-in defaults.
    ///
    /// This is a convenience for tests and bootstrapping UI state before user
    /// config has been loaded. It should not be used as a fallback after
    /// parsing `TuiKeymap`, because doing so would ignore explicit user
    /// unbindings and conflict diagnostics.
    pub(crate) fn defaults() -> Self {
        static DEFAULTS: std::sync::OnceLock<RuntimeKeymap> = std::sync::OnceLock::new();

        DEFAULTS
            .get_or_init(|| {
                Self::from_config(&TuiKeymap::default()).unwrap_or_else(|error| {
                    panic!("built-in keymap defaults must be valid: {error}")
                })
            })
            .clone()
    }

    /// Resolve a runtime keymap from config, applying precedence and validation.
    ///
    /// Returns an error when:
    ///
    /// 1. A keybinding spec cannot be parsed.
    /// 2. A context has ambiguous bindings (same key assigned to multiple actions).
    ///
    /// The error text includes the relevant config path and a concrete next step.
    /// Calling code should not merge bindings across unrelated contexts before
    /// dispatch, or conflict guarantees from this resolver no longer hold.
    pub(crate) fn from_config(keymap: &TuiKeymap) -> Result<Self, String> {
        let defaults = Self::built_in_defaults();
        let chords = Arc::new(RuntimeChordKeymap::from_config(keymap)?);
        let side_toggle_default_is_shadowed = keymap.global.toggle_side_conversation.is_none()
            && ["ctrl-/", "ctrl-7"].into_iter().any(|alias| {
                configured_main_surface_alias_is_used(keymap, alias)
                    || configured_context_alias_is_used(&keymap.list, alias)
                    || configured_context_alias_is_used(&keymap.approval, alias)
            });
        let app = AppKeymap {
            open_agents: resolve_bindings(
                keymap.global.open_agents.as_ref(),
                &defaults.app.open_agents,
                "tui.keymap.global.open_agents",
            )?,
            open_transcript: resolve_bindings(
                keymap.global.open_transcript.as_ref(),
                &defaults.app.open_transcript,
                "tui.keymap.global.open_transcript",
            )?,
            open_external_editor: resolve_bindings(
                keymap.global.open_external_editor.as_ref(),
                &defaults.app.open_external_editor,
                "tui.keymap.global.open_external_editor",
            )?,
            copy: resolve_bindings(
                keymap.global.copy.as_ref(),
                &defaults.app.copy,
                "tui.keymap.global.copy",
            )?,
            clear_terminal: resolve_bindings(
                keymap.global.clear_terminal.as_ref(),
                &defaults.app.clear_terminal,
                "tui.keymap.global.clear_terminal",
            )?,
            toggle_vim_mode: resolve_bindings(
                keymap.global.toggle_vim_mode.as_ref(),
                &defaults.app.toggle_vim_mode,
                "tui.keymap.global.toggle_vim_mode",
            )?,
            toggle_fast_mode: resolve_bindings(
                keymap.global.toggle_fast_mode.as_ref(),
                &defaults.app.toggle_fast_mode,
                "tui.keymap.global.toggle_fast_mode",
            )?,
            toggle_raw_output: resolve_bindings(
                keymap.global.toggle_raw_output.as_ref(),
                &defaults.app.toggle_raw_output,
                "tui.keymap.global.toggle_raw_output",
            )?,
            toggle_side_conversation: if side_toggle_default_is_shadowed {
                Vec::new()
            } else {
                resolve_bindings(
                    keymap.global.toggle_side_conversation.as_ref(),
                    &defaults.app.toggle_side_conversation,
                    "tui.keymap.global.toggle_side_conversation",
                )?
            },
        };

        let mut chat = ChatKeymap {
            interrupt_turn: resolve_bindings(
                keymap.chat.interrupt_turn.as_ref(),
                &defaults.chat.interrupt_turn,
                "tui.keymap.chat.interrupt_turn",
            )?,
            decrease_reasoning_effort: resolve_bindings(
                keymap.chat.decrease_reasoning_effort.as_ref(),
                &defaults.chat.decrease_reasoning_effort,
                "tui.keymap.chat.decrease_reasoning_effort",
            )?,
            increase_reasoning_effort: resolve_bindings(
                keymap.chat.increase_reasoning_effort.as_ref(),
                &defaults.chat.increase_reasoning_effort,
                "tui.keymap.chat.increase_reasoning_effort",
            )?,
            previous_permission_mode: resolve_local!(
                keymap,
                defaults,
                chat,
                previous_permission_mode
            ),
            next_permission_mode: resolve_local!(keymap, defaults, chat, next_permission_mode),
            edit_queued_message: resolve_bindings(
                keymap.chat.edit_queued_message.as_ref(),
                &defaults.chat.edit_queued_message,
                "tui.keymap.chat.edit_queued_message",
            )?,
            prompt_stack_back: resolve_local!(keymap, defaults, chat, prompt_stack_back),
            skip_question: resolve_local!(keymap, defaults, chat, skip_question),
        };

        let composer = ComposerKeymap {
            submit: resolve_with_global!(keymap, defaults, composer, submit),
            queue: resolve_with_global!(keymap, defaults, composer, queue),
            toggle_shortcuts: resolve_with_global!(keymap, defaults, composer, toggle_shortcuts),
            history_search_previous: resolve_local!(
                keymap,
                defaults,
                composer,
                history_search_previous
            ),
            history_search_next: resolve_local!(keymap, defaults, composer, history_search_next),
        };

        let editor = Arc::new(EditorKeymap {
            insert_newline: resolve_local!(keymap, defaults, editor, insert_newline),
            move_left: resolve_local!(keymap, defaults, editor, move_left),
            move_right: resolve_local!(keymap, defaults, editor, move_right),
            move_up: resolve_local!(keymap, defaults, editor, move_up),
            move_down: resolve_local!(keymap, defaults, editor, move_down),
            move_word_left: resolve_local!(keymap, defaults, editor, move_word_left),
            move_word_right: resolve_local!(keymap, defaults, editor, move_word_right),
            move_line_start: resolve_local!(keymap, defaults, editor, move_line_start),
            move_line_end: resolve_local!(keymap, defaults, editor, move_line_end),
            delete_backward: resolve_local!(keymap, defaults, editor, delete_backward),
            delete_forward: resolve_local!(keymap, defaults, editor, delete_forward),
            delete_backward_word: resolve_local!(keymap, defaults, editor, delete_backward_word),
            delete_forward_word: resolve_local!(keymap, defaults, editor, delete_forward_word),
            kill_line_start: resolve_local!(keymap, defaults, editor, kill_line_start),
            kill_whole_line: resolve_local!(keymap, defaults, editor, kill_whole_line),
            kill_line_end: resolve_local!(keymap, defaults, editor, kill_line_end),
            yank: resolve_local!(keymap, defaults, editor, yank),
        });

        let (vim_normal, vim_operator, vim_text_object) =
            resolve_vim::resolve_vim(keymap, &defaults, &chords, &mut chat)?;

        let pager = PagerKeymap {
            scroll_up: resolve_local!(keymap, defaults, pager, scroll_up),
            scroll_down: resolve_local!(keymap, defaults, pager, scroll_down),
            page_up: resolve_local!(keymap, defaults, pager, page_up),
            page_down: resolve_local!(keymap, defaults, pager, page_down),
            half_page_up: resolve_local!(keymap, defaults, pager, half_page_up),
            half_page_down: resolve_local!(keymap, defaults, pager, half_page_down),
            jump_top: resolve_local!(keymap, defaults, pager, jump_top),
            jump_bottom: resolve_local!(keymap, defaults, pager, jump_bottom),
            close: resolve_local!(keymap, defaults, pager, close),
            close_transcript: resolve_local!(keymap, defaults, pager, close_transcript),
            chord_hints: Arc::clone(&chords),
        };

        let resume_default_is_shadowed = keymap.agents.resume.is_none()
            && (configured_context_alias_is_used(&keymap.agents, "ctrl-o")
                || configured_context_alias_is_used(&keymap.list, "ctrl-o")
                || chords.bindings.iter().any(|binding| {
                    binding.action.context.overlaps(KeymapContext::Agents)
                        && binding.chord.prefix.parts()
                            == key_hint::ctrl(KeyCode::Char('o')).parts()
                }));
        let mut agents = AgentsKeymap {
            resume: if resume_default_is_shadowed {
                Vec::new()
            } else {
                resolve_local!(keymap, defaults, agents, resume)
            },
            search: resolve_local!(keymap, defaults, agents, search),
            new_task: resolve_local!(keymap, defaults, agents, new_task),
            rename: resolve_local!(keymap, defaults, agents, rename),
            stop: resolve_local!(keymap, defaults, agents, stop),
            toggle_grouping: resolve_local!(keymap, defaults, agents, toggle_grouping),
            chord_hints: Arc::clone(&chords),
        };

        let approval = ApprovalKeymap {
            open_fullscreen: resolve_local!(keymap, defaults, approval, open_fullscreen),
            open_thread: resolve_local!(keymap, defaults, approval, open_thread),
            approve: resolve_local!(keymap, defaults, approval, approve),
            approve_for_session: resolve_local!(keymap, defaults, approval, approve_for_session),
            approve_for_prefix: resolve_local!(keymap, defaults, approval, approve_for_prefix),
            deny: resolve_local!(keymap, defaults, approval, deny),
            decline: resolve_local!(keymap, defaults, approval, decline),
            cancel: resolve_local!(keymap, defaults, approval, cancel),
            chord_hints: Arc::clone(&chords),
        };

        let list_move_up = resolve_local!(keymap, defaults, list, move_up);
        let list_move_down = resolve_local!(keymap, defaults, list, move_down);
        let list_accept = resolve_local!(keymap, defaults, list, accept);
        let list_cancel = resolve_local!(keymap, defaults, list, cancel);
        let configured_bindings_to_preserve = configured_bindings_to_preserve([
            (
                keymap.global.open_agents.as_ref(),
                app.open_agents.as_slice(),
            ),
            (
                keymap.global.open_transcript.as_ref(),
                app.open_transcript.as_slice(),
            ),
            (
                keymap.global.open_external_editor.as_ref(),
                app.open_external_editor.as_slice(),
            ),
            (keymap.global.copy.as_ref(), app.copy.as_slice()),
            (
                keymap.global.clear_terminal.as_ref(),
                app.clear_terminal.as_slice(),
            ),
            (
                keymap.global.toggle_vim_mode.as_ref(),
                app.toggle_vim_mode.as_slice(),
            ),
            (
                keymap.global.toggle_fast_mode.as_ref(),
                app.toggle_fast_mode.as_slice(),
            ),
            (
                keymap.global.toggle_raw_output.as_ref(),
                app.toggle_raw_output.as_slice(),
            ),
            (
                keymap.global.toggle_side_conversation.as_ref(),
                app.toggle_side_conversation.as_slice(),
            ),
            (keymap.list.move_up.as_ref(), list_move_up.as_slice()),
            (keymap.list.move_down.as_ref(), list_move_down.as_slice()),
            (keymap.list.accept.as_ref(), list_accept.as_slice()),
            (keymap.list.cancel.as_ref(), list_cancel.as_slice()),
            (
                keymap.approval.open_fullscreen.as_ref(),
                approval.open_fullscreen.as_slice(),
            ),
            (
                keymap.approval.open_thread.as_ref(),
                approval.open_thread.as_slice(),
            ),
            (
                keymap.approval.approve.as_ref(),
                approval.approve.as_slice(),
            ),
            (
                keymap.approval.approve_for_session.as_ref(),
                approval.approve_for_session.as_slice(),
            ),
            (
                keymap.approval.approve_for_prefix.as_ref(),
                approval.approve_for_prefix.as_slice(),
            ),
            (keymap.approval.deny.as_ref(), approval.deny.as_slice()),
            (
                keymap.approval.decline.as_ref(),
                approval.decline.as_slice(),
            ),
            (keymap.approval.cancel.as_ref(), approval.cancel.as_slice()),
        ]);

        let list = ListKeymap {
            move_up: list_move_up,
            move_down: list_move_down,
            move_left: resolve_new_default_bindings(
                keymap.list.move_left.as_ref(),
                &defaults.list.move_left,
                &configured_bindings_to_preserve,
                "tui.keymap.list.move_left",
            )?,
            move_right: resolve_new_default_bindings(
                keymap.list.move_right.as_ref(),
                &defaults.list.move_right,
                &configured_bindings_to_preserve,
                "tui.keymap.list.move_right",
            )?,
            page_up: resolve_new_default_bindings(
                keymap.list.page_up.as_ref(),
                &defaults.list.page_up,
                &configured_bindings_to_preserve,
                "tui.keymap.list.page_up",
            )?,
            page_down: resolve_new_default_bindings(
                keymap.list.page_down.as_ref(),
                &defaults.list.page_down,
                &configured_bindings_to_preserve,
                "tui.keymap.list.page_down",
            )?,
            jump_top: resolve_new_default_bindings(
                keymap.list.jump_top.as_ref(),
                &defaults.list.jump_top,
                &configured_bindings_to_preserve,
                "tui.keymap.list.jump_top",
            )?,
            jump_bottom: resolve_new_default_bindings(
                keymap.list.jump_bottom.as_ref(),
                &defaults.list.jump_bottom,
                &configured_bindings_to_preserve,
                "tui.keymap.list.jump_bottom",
            )?,
            accept: list_accept,
            cancel: list_cancel,
            chord_hints: Arc::clone(&chords),
        };

        for (configured, bindings) in [
            (keymap.agents.resume.as_ref(), &mut agents.resume),
            (keymap.agents.search.as_ref(), &mut agents.search),
            (keymap.agents.new_task.as_ref(), &mut agents.new_task),
            (keymap.agents.rename.as_ref(), &mut agents.rename),
            (keymap.agents.stop.as_ref(), &mut agents.stop),
            (
                keymap.agents.toggle_grouping.as_ref(),
                &mut agents.toggle_grouping,
            ),
        ] {
            if configured.is_none() {
                bindings.retain(|binding| {
                    !chords.bindings.iter().any(|chord| {
                        chord.action.context == KeymapContext::List
                            && chord.chord.prefix == *binding
                    })
                });
            }
        }

        let mut resolved = Self {
            app,
            chords,
            chat,
            composer,
            editor,
            vim_normal,
            vim_operator,
            vim_search: VimSearchKeymap {
                forward: resolve_local!(keymap, defaults, vim_search, forward),
                backward: resolve_local!(keymap, defaults, vim_search, backward),
                next: resolve_local!(keymap, defaults, vim_search, next),
                previous: resolve_local!(keymap, defaults, vim_search, previous),
            },
            vim_text_object,
            pager,
            list,
            agents,
            approval,
        };

        let configured: Vec<_> = runtime_action_bindings(&resolved)
            .filter(|action| {
                action.id.context.overlaps(KeymapContext::VimNormal)
                    && bindings::configured_binding_for_action(keymap, action.id)
                        .is_some_and(Option::is_some)
            })
            .flat_map(|action| action.bindings.iter().copied())
            .collect();
        for (setting, bindings) in [
            (
                keymap.vim_normal.undo.as_ref(),
                &mut resolved.vim_normal.undo,
            ),
            (
                keymap.vim_normal.redo.as_ref(),
                &mut resolved.vim_normal.redo,
            ),
            (
                keymap.vim_normal.enter_replace_mode.as_ref(),
                &mut resolved.vim_normal.enter_replace_mode,
            ),
        ] {
            if setting.is_none() {
                bindings.retain(|binding| {
                    let (code, modifiers) = binding.parts();
                    let event = KeyEvent::new(code, modifiers);
                    !configured.is_pressed(event)
                        && !resolved.chords.bindings.iter().any(|chord| {
                            chord.action.context.overlaps(KeymapContext::VimNormal)
                                && chord.chord.prefix.is_press(event)
                        })
                });
            }
        }
        resolved.configure_vim_search(keymap)?;
        resolved.validate_conflicts()?;
        chords::validate_chord_conflicts(&resolved)?;
        chords::install_dispatch_bindings(&mut resolved)?;
        Ok(resolved)
    }

    /// Resolve the visible primary shortcut from configured declaration order.
    pub(crate) fn primary_hint(
        &self,
        context: KeymapContext,
        action: &'static str,
    ) -> Option<ShortcutHint> {
        let action_id = keymap_action_id(context.config_name(), action)?;
        let bindings = bindings_for_action(self, context.config_name(), action)?;
        self.chords.primary_hint(action_id, bindings)
    }
}

/// Resolve one action with context, global, then default precedence.
///
/// A configured empty list explicitly unbinds the action.
pub(super) fn resolve_bindings_with_global_fallback(
    configured: Option<&KeybindingsSpec>,
    global: Option<&KeybindingsSpec>,
    fallback: &[KeyBinding],
    path: &str,
) -> Result<Vec<KeyBinding>, String> {
    if let Some(configured) = configured {
        return parse_bindings(configured, path);
    }
    if let Some(global) = global {
        return parse_bindings(global, path);
    }
    Ok(fallback.to_vec())
}

/// Resolve one action binding in a context without global fallback.
///
/// Missing values inherit from the built-in fallback; configured values, including
/// empty lists, replace that fallback for the action.
pub(super) fn resolve_bindings(
    configured: Option<&KeybindingsSpec>,
    fallback: &[KeyBinding],
    path: &str,
) -> Result<Vec<KeyBinding>, String> {
    let Some(spec) = configured else {
        return Ok(fallback.to_vec());
    };
    parse_bindings(spec, path)
}

pub(super) fn configured_bindings_to_preserve<const N: usize>(
    pairs: [(Option<&KeybindingsSpec>, &[KeyBinding]); N],
) -> Vec<KeyBinding> {
    let mut configured_bindings = Vec::new();
    for (configured, resolved) in pairs {
        if configured.is_none() {
            continue;
        }
        for binding in resolved {
            if !configured_bindings.contains(binding) {
                configured_bindings.push(*binding);
            }
        }
    }
    configured_bindings
}

pub(super) fn configured_main_surface_alias_is_used(keymap: &TuiKeymap, alias: &str) -> bool {
    let mut global = keymap.global.clone();
    if keymap.composer.submit.is_some() {
        global.submit = None;
    }
    if keymap.composer.queue.is_some() {
        global.queue = None;
    }
    if keymap.composer.toggle_shortcuts.is_some() {
        global.toggle_shortcuts = None;
    }

    // Reasoning shortcuts run before composer/editor key handling, so fallback
    // aliases must yield to any explicit binding on the same main-surface input
    // path.
    configured_context_alias_is_used(&global, alias)
        || configured_context_alias_is_used(&keymap.chat, alias)
        || configured_context_alias_is_used(&keymap.composer, alias)
        || configured_context_alias_is_used(&keymap.editor, alias)
        || configured_context_alias_is_used(&keymap.vim_normal, alias)
        || configured_context_alias_is_used(&keymap.vim_operator, alias)
        || configured_context_alias_is_used(&keymap.vim_text_object, alias)
}

pub(super) fn configured_context_alias_is_used(context: &impl Serialize, alias: &str) -> bool {
    let Ok(value) = serde_json::to_value(context) else {
        return false;
    };
    keymap_value_contains_alias(&value, alias)
}

pub(super) fn keymap_value_contains_alias(value: &serde_json::Value, alias: &str) -> bool {
    match value {
        serde_json::Value::String(value) => parse_keybinding(value)
            .zip(parse_keybinding(alias))
            .is_some_and(|(a, b)| a.normalized_parts() == b.normalized_parts()),
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| keymap_value_contains_alias(value, alias)),
        serde_json::Value::Object(values) => values
            .values()
            .any(|value| keymap_value_contains_alias(value, alias)),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) | serde_json::Value::Null => {
            false
        }
    }
}

pub(super) fn resolve_new_default_bindings(
    configured: Option<&KeybindingsSpec>,
    fallback: &[KeyBinding],
    configured_bindings_to_preserve: &[KeyBinding],
    path: &str,
) -> Result<Vec<KeyBinding>, String> {
    let Some(spec) = configured else {
        return Ok(fallback
            .iter()
            .copied()
            .filter(|binding| !configured_bindings_to_preserve.contains(binding))
            .collect());
    };
    parse_bindings(spec, path)
}

/// Parse one keybinding value (`string` or `list[string]`) into concrete bindings.
///
/// Duplicate entries are de-duplicated while preserving first-seen order so the
/// first key can remain the primary UI hint.
pub(super) fn parse_bindings(
    spec: &KeybindingsSpec,
    path: &str,
) -> Result<Vec<KeyBinding>, String> {
    let mut parsed = Vec::new();
    for raw in spec.specs() {
        if raw.as_str().contains(' ') {
            continue;
        }
        let binding = parse_keybinding(raw.as_str()).ok_or_else(|| {
            format!(
                "Invalid `{path}` = `{}`. Use values like `ctrl-a`, `shift-enter`, or `page-down`. \
See the Codex keymap documentation for supported actions and examples.",
                raw.as_str()
            )
        })?;

        if !parsed
            .iter()
            .any(|previous: &KeyBinding| previous.normalized_parts() == binding.normalized_parts())
        {
            parsed.push(binding);
        }
    }
    Ok(parsed)
}

/// Parse one normalized keybinding spec such as `ctrl-a` or `shift-enter`.
///
/// Specs are expected to be normalized by config deserialization, but this
/// parser remains strict to keep runtime error messages precise.
pub(super) fn parse_keybinding(spec: &str) -> Option<KeyBinding> {
    let mut parts = spec.split('-');
    let mut modifiers = KeyModifiers::NONE;
    let mut key_name = None;

    for part in parts.by_ref() {
        match part {
            "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            other => {
                key_name = Some(other.to_string());
                break;
            }
        }
    }

    let mut key_name = key_name?;
    for trailing in parts {
        key_name.push('-');
        key_name.push_str(trailing);
    }

    let key = match key_name.as_str() {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "esc" => KeyCode::Esc,
        "delete" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "page-up" => KeyCode::PageUp,
        "page-down" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        "minus" => KeyCode::Char('-'),
        other if other.len() == 1 => KeyCode::Char(char::from(other.as_bytes()[0])),
        other if other.starts_with('f') => {
            let number = other[1..].parse::<u8>().ok()?;
            if (1..=MAX_FUNCTION_KEY).contains(&number) {
                KeyCode::F(number)
            } else {
                return None;
            }
        }
        _ => return None,
    };

    Some(KeyBinding::new(key, modifiers))
}
