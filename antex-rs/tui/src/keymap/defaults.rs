use super::*;

impl RuntimeKeymap {
    /// Built-in keymap defaults.
    ///
    /// Some actions include compatibility variants because terminals disagree
    /// on whether Shift is preserved for printable and control chords.
    pub(super) fn built_in_defaults() -> Self {
        Self {
            app: AppKeymap {
                open_agents: default_bindings![],
                open_transcript: default_bindings![ctrl(KeyCode::Char('t'))],
                open_external_editor: default_bindings![ctrl(KeyCode::Char('g'))],
                copy: default_bindings![ctrl(KeyCode::Char('o'))],
                clear_terminal: default_bindings![ctrl(KeyCode::Char('l'))],
                toggle_vim_mode: default_bindings![],
                toggle_fast_mode: default_bindings![],
                toggle_raw_output: default_bindings![alt(KeyCode::Char('r'))],
                toggle_side_conversation: default_bindings![ctrl(KeyCode::Char('/'))],
            },
            chords: Arc::default(),
            chat: ChatKeymap {
                interrupt_turn: default_bindings![plain(KeyCode::Esc)],
                decrease_reasoning_effort: default_bindings![
                    alt(KeyCode::Char(',')),
                    shift(KeyCode::Down)
                ],
                increase_reasoning_effort: default_bindings![
                    alt(KeyCode::Char('.')),
                    shift(KeyCode::Up)
                ],
                previous_permission_mode: default_bindings![],
                next_permission_mode: default_bindings![],
                edit_queued_message: default_bindings![alt(KeyCode::Up), shift(KeyCode::Left)],
                prompt_stack_back: default_bindings![alt(KeyCode::Down), shift(KeyCode::Right)],
                skip_question: default_bindings![ctrl(KeyCode::Char(']'))],
            },
            composer: ComposerKeymap {
                submit: default_bindings![plain(KeyCode::Enter)],
                queue: default_bindings![plain(KeyCode::Tab)],
                toggle_shortcuts: default_bindings![
                    plain(KeyCode::Char('?')),
                    shift(KeyCode::Char('?'))
                ],
                history_search_previous: default_bindings![ctrl(KeyCode::Char('r'))],
                history_search_next: default_bindings![ctrl(KeyCode::Char('s'))],
            },
            editor: Arc::new(EditorKeymap {
                insert_newline: default_bindings![
                    ctrl(KeyCode::Char('j')),
                    ctrl(KeyCode::Char('m')),
                    plain(KeyCode::Enter),
                    shift(KeyCode::Enter),
                    alt(KeyCode::Enter)
                ],
                move_left: default_bindings![plain(KeyCode::Left), ctrl(KeyCode::Char('b'))],
                move_right: default_bindings![plain(KeyCode::Right), ctrl(KeyCode::Char('f'))],
                move_up: default_bindings![plain(KeyCode::Up), ctrl(KeyCode::Char('p'))],
                move_down: default_bindings![plain(KeyCode::Down), ctrl(KeyCode::Char('n'))],
                move_word_left: default_bindings![
                    alt(KeyCode::Char('b')),
                    raw(KeyBinding::new(KeyCode::Left, KeyModifiers::ALT)),
                    raw(KeyBinding::new(KeyCode::Left, KeyModifiers::CONTROL))
                ],
                move_word_right: default_bindings![
                    alt(KeyCode::Char('f')),
                    raw(KeyBinding::new(KeyCode::Right, KeyModifiers::ALT)),
                    raw(KeyBinding::new(KeyCode::Right, KeyModifiers::CONTROL))
                ],
                move_line_start: default_bindings![plain(KeyCode::Home), ctrl(KeyCode::Char('a'))],
                move_line_end: default_bindings![plain(KeyCode::End), ctrl(KeyCode::Char('e'))],
                delete_backward: default_bindings![
                    plain(KeyCode::Backspace),
                    shift(KeyCode::Backspace),
                    ctrl(KeyCode::Char('h'))
                ],
                delete_forward: default_bindings![
                    plain(KeyCode::Delete),
                    shift(KeyCode::Delete),
                    ctrl(KeyCode::Char('d'))
                ],
                delete_backward_word: default_bindings![
                    alt(KeyCode::Backspace),
                    ctrl(KeyCode::Backspace),
                    raw(KeyBinding::new(
                        KeyCode::Backspace,
                        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                    )),
                    ctrl(KeyCode::Char('w')),
                    raw(KeyBinding::new(
                        KeyCode::Char('h'),
                        KeyModifiers::CONTROL | KeyModifiers::ALT,
                    ))
                ],
                delete_forward_word: default_bindings![
                    alt(KeyCode::Delete),
                    ctrl(KeyCode::Delete),
                    raw(KeyBinding::new(
                        KeyCode::Delete,
                        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                    )),
                    alt(KeyCode::Char('d'))
                ],
                kill_line_start: default_bindings![ctrl(KeyCode::Char('u'))],
                kill_whole_line: default_bindings![],
                kill_line_end: default_bindings![ctrl(KeyCode::Char('k'))],
                yank: default_bindings![ctrl(KeyCode::Char('y'))],
            }),
            vim_normal: VimNormalKeymap {
                enter_insert: default_bindings![plain(KeyCode::Char('i')), plain(KeyCode::Insert)],
                append_after_cursor: default_bindings![plain(KeyCode::Char('a'))],
                append_line_end: default_bindings![
                    shift(KeyCode::Char('a')),
                    plain(KeyCode::Char('A'))
                ],
                insert_line_start: default_bindings![
                    shift(KeyCode::Char('i')),
                    plain(KeyCode::Char('I'))
                ],
                open_line_below: default_bindings![plain(KeyCode::Char('o'))],
                open_line_above: default_bindings![
                    shift(KeyCode::Char('o')),
                    plain(KeyCode::Char('O'))
                ],
                enter_replace_mode: default_bindings![
                    shift(KeyCode::Char('r')),
                    plain(KeyCode::Char('R'))
                ],
                move_left: default_bindings![plain(KeyCode::Char('h')), plain(KeyCode::Left)],
                move_right: default_bindings![plain(KeyCode::Char('l')), plain(KeyCode::Right)],
                move_up: default_bindings![plain(KeyCode::Char('k')), plain(KeyCode::Up)],
                move_down: default_bindings![plain(KeyCode::Char('j')), plain(KeyCode::Down)],
                move_word_forward: default_bindings![plain(KeyCode::Char('w'))],
                move_word_backward: default_bindings![plain(KeyCode::Char('b'))],
                move_word_end: default_bindings![plain(KeyCode::Char('e'))],
                move_line_start: default_bindings![plain(KeyCode::Char('0'))],
                move_line_end: default_bindings![
                    plain(KeyCode::Char('$')),
                    shift(KeyCode::Char('$'))
                ],
                find_forward: default_bindings![plain(KeyCode::Char('f'))],
                find_backward: default_bindings![shift(KeyCode::Char('f'))],
                till_forward: default_bindings![plain(KeyCode::Char('t'))],
                till_backward: default_bindings![shift(KeyCode::Char('t'))],
                jump_top: default_bindings![],
                jump_bottom: default_bindings![
                    shift(KeyCode::Char('g')),
                    plain(KeyCode::Char('G'))
                ],
                delete_char: default_bindings![plain(KeyCode::Char('x'))],
                replace_char: default_bindings![plain(KeyCode::Char('r'))],
                repeat_last_change: default_bindings![plain(KeyCode::Char('.'))],
                substitute_char: default_bindings![plain(KeyCode::Char('s'))],
                delete_to_line_end: default_bindings![
                    shift(KeyCode::Char('d')),
                    plain(KeyCode::Char('D'))
                ],
                change_to_line_end: default_bindings![
                    shift(KeyCode::Char('c')),
                    plain(KeyCode::Char('C'))
                ],
                yank_line: default_bindings![shift(KeyCode::Char('y')), plain(KeyCode::Char('Y'))],
                paste_after: default_bindings![plain(KeyCode::Char('p'))],
                start_delete_operator: default_bindings![plain(KeyCode::Char('d'))],
                start_yank_operator: default_bindings![plain(KeyCode::Char('y'))],
                start_change_operator: default_bindings![plain(KeyCode::Char('c'))],
                undo: default_bindings![plain(KeyCode::Char('u'))],
                redo: default_bindings![ctrl(KeyCode::Char('r'))],
                cancel_operator: default_bindings![plain(KeyCode::Esc)],
            },
            vim_search: VimSearchKeymap::default(),
            vim_operator: VimOperatorKeymap {
                delete_line: default_bindings![plain(KeyCode::Char('d'))],
                yank_line: default_bindings![plain(KeyCode::Char('y'))],
                motion_left: default_bindings![plain(KeyCode::Char('h'))],
                motion_right: default_bindings![plain(KeyCode::Char('l'))],
                motion_up: default_bindings![plain(KeyCode::Char('k'))],
                motion_down: default_bindings![plain(KeyCode::Char('j'))],
                motion_word_forward: default_bindings![plain(KeyCode::Char('w'))],
                motion_word_backward: default_bindings![plain(KeyCode::Char('b'))],
                motion_word_end: default_bindings![plain(KeyCode::Char('e'))],
                motion_line_start: default_bindings![plain(KeyCode::Char('0'))],
                motion_line_end: default_bindings![
                    plain(KeyCode::Char('$')),
                    shift(KeyCode::Char('$'))
                ],
                motion_find_forward: default_bindings![plain(KeyCode::Char('f'))],
                motion_find_backward: default_bindings![shift(KeyCode::Char('f'))],
                motion_till_forward: default_bindings![plain(KeyCode::Char('t'))],
                motion_till_backward: default_bindings![shift(KeyCode::Char('t'))],
                motion_jump_top: default_bindings![],
                motion_jump_bottom: default_bindings![
                    shift(KeyCode::Char('g')),
                    plain(KeyCode::Char('G'))
                ],
                select_inner_text_object: default_bindings![plain(KeyCode::Char('i'))],
                select_around_text_object: default_bindings![plain(KeyCode::Char('a'))],
                cancel: default_bindings![plain(KeyCode::Esc)],
            },
            vim_text_object: VimTextObjectKeymap {
                word: default_bindings![plain(KeyCode::Char('w'))],
                big_word: default_bindings![shift(KeyCode::Char('w')), plain(KeyCode::Char('W'))],
                parentheses: default_bindings![
                    plain(KeyCode::Char('(')),
                    shift(KeyCode::Char('(')),
                    plain(KeyCode::Char(')')),
                    shift(KeyCode::Char(')')),
                    plain(KeyCode::Char('b'))
                ],
                brackets: default_bindings![plain(KeyCode::Char('[')), plain(KeyCode::Char(']'))],
                braces: default_bindings![
                    plain(KeyCode::Char('{')),
                    shift(KeyCode::Char('{')),
                    plain(KeyCode::Char('}')),
                    shift(KeyCode::Char('}')),
                    shift(KeyCode::Char('b')),
                    plain(KeyCode::Char('B'))
                ],
                double_quote: default_bindings![
                    plain(KeyCode::Char('"')),
                    shift(KeyCode::Char('"'))
                ],
                single_quote: default_bindings![plain(KeyCode::Char('\''))],
                backtick: default_bindings![plain(KeyCode::Char('`'))],
                cancel: default_bindings![plain(KeyCode::Esc)],
            },
            pager: PagerKeymap {
                scroll_up: default_bindings![plain(KeyCode::Up), plain(KeyCode::Char('k'))],
                scroll_down: default_bindings![plain(KeyCode::Down), plain(KeyCode::Char('j'))],
                page_up: default_bindings![
                    plain(KeyCode::PageUp),
                    shift(KeyCode::Char(' ')),
                    ctrl(KeyCode::Char('b'))
                ],
                page_down: default_bindings![
                    plain(KeyCode::PageDown),
                    plain(KeyCode::Char(' ')),
                    ctrl(KeyCode::Char('f'))
                ],
                half_page_up: default_bindings![ctrl(KeyCode::Char('u'))],
                half_page_down: default_bindings![ctrl(KeyCode::Char('d'))],
                jump_top: default_bindings![plain(KeyCode::Home)],
                jump_bottom: default_bindings![plain(KeyCode::End)],
                close: default_bindings![plain(KeyCode::Char('q')), ctrl(KeyCode::Char('c'))],
                close_transcript: default_bindings![ctrl(KeyCode::Char('t'))],
                chord_hints: Arc::default(),
            },
            list: ListKeymap {
                move_up: default_bindings![
                    plain(KeyCode::Up),
                    ctrl(KeyCode::Char('p')),
                    ctrl(KeyCode::Char('k')),
                    plain(KeyCode::Char('k'))
                ],
                move_down: default_bindings![
                    plain(KeyCode::Down),
                    ctrl(KeyCode::Char('n')),
                    ctrl(KeyCode::Char('j')),
                    plain(KeyCode::Char('j'))
                ],
                move_left: default_bindings![plain(KeyCode::Left), ctrl(KeyCode::Char('h'))],
                move_right: default_bindings![plain(KeyCode::Right), ctrl(KeyCode::Char('l'))],
                page_up: default_bindings![plain(KeyCode::PageUp), ctrl(KeyCode::Char('b'))],
                page_down: default_bindings![plain(KeyCode::PageDown), ctrl(KeyCode::Char('f'))],
                jump_top: default_bindings![plain(KeyCode::Home)],
                jump_bottom: default_bindings![plain(KeyCode::End)],
                accept: default_bindings![plain(KeyCode::Enter)],
                cancel: default_bindings![plain(KeyCode::Esc)],
                chord_hints: Arc::default(),
            },
            agents: AgentsKeymap {
                resume: default_bindings![ctrl(KeyCode::Char('o'))],
                search: default_bindings![ctrl(KeyCode::Char('f'))],
                new_task: default_bindings![ctrl(KeyCode::Char('n'))],
                rename: default_bindings![ctrl(KeyCode::Char('r'))],
                stop: default_bindings![ctrl(KeyCode::Char('x'))],
                toggle_grouping: default_bindings![ctrl(KeyCode::Char('s'))],
                chord_hints: Arc::default(),
            },
            approval: ApprovalKeymap {
                open_fullscreen: default_bindings![
                    ctrl(KeyCode::Char('a')),
                    raw(KeyBinding::new(
                        KeyCode::Char('a'),
                        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                    ))
                ],
                open_thread: default_bindings![plain(KeyCode::Char('o'))],
                approve: default_bindings![plain(KeyCode::Char('y'))],
                approve_for_session: default_bindings![plain(KeyCode::Char('a'))],
                approve_for_prefix: default_bindings![plain(KeyCode::Char('p'))],
                deny: default_bindings![plain(KeyCode::Char('d'))],
                decline: default_bindings![plain(KeyCode::Esc), plain(KeyCode::Char('n'))],
                cancel: default_bindings![plain(KeyCode::Char('c'))],
                chord_hints: Arc::default(),
            },
        }
    }
}
