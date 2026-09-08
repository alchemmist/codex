use super::*;

pub(super) fn resolve_vim(
    keymap: &TuiKeymap,
    defaults: &RuntimeKeymap,
    chords: &RuntimeChordKeymap,
    chat: &mut ChatKeymap,
) -> Result<(VimNormalKeymap, VimOperatorKeymap, VimTextObjectKeymap), String> {
    let mut vim_normal = VimNormalKeymap {
        enter_insert: resolve_local!(keymap, defaults, vim_normal, enter_insert),
        append_after_cursor: resolve_local!(keymap, defaults, vim_normal, append_after_cursor),
        append_line_end: resolve_local!(keymap, defaults, vim_normal, append_line_end),
        insert_line_start: resolve_local!(keymap, defaults, vim_normal, insert_line_start),
        open_line_below: resolve_local!(keymap, defaults, vim_normal, open_line_below),
        open_line_above: resolve_local!(keymap, defaults, vim_normal, open_line_above),
        enter_replace_mode: resolve_local!(keymap, defaults, vim_normal, enter_replace_mode),
        move_left: resolve_local!(keymap, defaults, vim_normal, move_left),
        move_right: resolve_local!(keymap, defaults, vim_normal, move_right),
        move_up: resolve_local!(keymap, defaults, vim_normal, move_up),
        move_down: resolve_local!(keymap, defaults, vim_normal, move_down),
        move_word_forward: resolve_local!(keymap, defaults, vim_normal, move_word_forward),
        move_word_backward: resolve_local!(keymap, defaults, vim_normal, move_word_backward),
        move_word_end: resolve_local!(keymap, defaults, vim_normal, move_word_end),
        move_line_start: resolve_local!(keymap, defaults, vim_normal, move_line_start),
        move_line_end: resolve_local!(keymap, defaults, vim_normal, move_line_end),
        find_forward: resolve_local!(keymap, defaults, vim_normal, find_forward),
        find_backward: resolve_local!(keymap, defaults, vim_normal, find_backward),
        till_forward: resolve_local!(keymap, defaults, vim_normal, till_forward),
        till_backward: resolve_local!(keymap, defaults, vim_normal, till_backward),
        jump_top: resolve_local!(keymap, defaults, vim_normal, jump_top),
        jump_bottom: resolve_local!(keymap, defaults, vim_normal, jump_bottom),
        delete_char: resolve_local!(keymap, defaults, vim_normal, delete_char),
        replace_char: resolve_local!(keymap, defaults, vim_normal, replace_char),
        repeat_last_change: resolve_local!(keymap, defaults, vim_normal, repeat_last_change),
        substitute_char: resolve_local!(keymap, defaults, vim_normal, substitute_char),
        delete_to_line_end: resolve_local!(keymap, defaults, vim_normal, delete_to_line_end),
        change_to_line_end: resolve_local!(keymap, defaults, vim_normal, change_to_line_end),
        yank_line: resolve_local!(keymap, defaults, vim_normal, yank_line),
        paste_after: resolve_local!(keymap, defaults, vim_normal, paste_after),
        start_delete_operator: resolve_local!(keymap, defaults, vim_normal, start_delete_operator),
        start_yank_operator: resolve_local!(keymap, defaults, vim_normal, start_yank_operator),
        start_change_operator: resolve_local!(keymap, defaults, vim_normal, start_change_operator),
        undo: resolve_local!(keymap, defaults, vim_normal, undo),
        redo: resolve_local!(keymap, defaults, vim_normal, redo),
        cancel_operator: resolve_local!(keymap, defaults, vim_normal, cancel_operator),
    };

    let configured_vim_normal_bindings_to_preserve = configured_bindings_to_preserve([
        (
            keymap.vim_normal.enter_insert.as_ref(),
            vim_normal.enter_insert.as_slice(),
        ),
        (
            keymap.vim_normal.append_after_cursor.as_ref(),
            vim_normal.append_after_cursor.as_slice(),
        ),
        (
            keymap.vim_normal.append_line_end.as_ref(),
            vim_normal.append_line_end.as_slice(),
        ),
        (
            keymap.vim_normal.insert_line_start.as_ref(),
            vim_normal.insert_line_start.as_slice(),
        ),
        (
            keymap.vim_normal.open_line_below.as_ref(),
            vim_normal.open_line_below.as_slice(),
        ),
        (
            keymap.vim_normal.open_line_above.as_ref(),
            vim_normal.open_line_above.as_slice(),
        ),
        (
            keymap.vim_normal.enter_replace_mode.as_ref(),
            vim_normal.enter_replace_mode.as_slice(),
        ),
        (
            keymap.vim_normal.move_left.as_ref(),
            vim_normal.move_left.as_slice(),
        ),
        (
            keymap.vim_normal.move_right.as_ref(),
            vim_normal.move_right.as_slice(),
        ),
        (
            keymap.vim_normal.move_up.as_ref(),
            vim_normal.move_up.as_slice(),
        ),
        (
            keymap.vim_normal.move_down.as_ref(),
            vim_normal.move_down.as_slice(),
        ),
        (
            keymap.vim_normal.move_word_forward.as_ref(),
            vim_normal.move_word_forward.as_slice(),
        ),
        (
            keymap.vim_normal.move_word_backward.as_ref(),
            vim_normal.move_word_backward.as_slice(),
        ),
        (
            keymap.vim_normal.move_word_end.as_ref(),
            vim_normal.move_word_end.as_slice(),
        ),
        (
            keymap.vim_normal.move_line_start.as_ref(),
            vim_normal.move_line_start.as_slice(),
        ),
        (
            keymap.vim_normal.move_line_end.as_ref(),
            vim_normal.move_line_end.as_slice(),
        ),
        (
            keymap.vim_normal.find_forward.as_ref(),
            vim_normal.find_forward.as_slice(),
        ),
        (
            keymap.vim_normal.find_backward.as_ref(),
            vim_normal.find_backward.as_slice(),
        ),
        (
            keymap.vim_normal.till_forward.as_ref(),
            vim_normal.till_forward.as_slice(),
        ),
        (
            keymap.vim_normal.till_backward.as_ref(),
            vim_normal.till_backward.as_slice(),
        ),
        (
            keymap.vim_normal.jump_top.as_ref(),
            vim_normal.jump_top.as_slice(),
        ),
        (
            keymap.vim_normal.jump_bottom.as_ref(),
            vim_normal.jump_bottom.as_slice(),
        ),
        (
            keymap.vim_normal.delete_char.as_ref(),
            vim_normal.delete_char.as_slice(),
        ),
        (
            keymap.vim_normal.replace_char.as_ref(),
            vim_normal.replace_char.as_slice(),
        ),
        (
            keymap.vim_normal.substitute_char.as_ref(),
            vim_normal.substitute_char.as_slice(),
        ),
        (
            keymap.vim_normal.repeat_last_change.as_ref(),
            vim_normal.repeat_last_change.as_slice(),
        ),
        (
            keymap.vim_normal.change_to_line_end.as_ref(),
            vim_normal.change_to_line_end.as_slice(),
        ),
        (
            keymap.vim_normal.delete_to_line_end.as_ref(),
            vim_normal.delete_to_line_end.as_slice(),
        ),
        (
            keymap.vim_normal.yank_line.as_ref(),
            vim_normal.yank_line.as_slice(),
        ),
        (
            keymap.vim_normal.paste_after.as_ref(),
            vim_normal.paste_after.as_slice(),
        ),
        (
            keymap.vim_normal.start_delete_operator.as_ref(),
            vim_normal.start_delete_operator.as_slice(),
        ),
        (
            keymap.vim_normal.start_yank_operator.as_ref(),
            vim_normal.start_yank_operator.as_slice(),
        ),
        (
            keymap.vim_normal.start_change_operator.as_ref(),
            vim_normal.start_change_operator.as_slice(),
        ),
        (keymap.vim_normal.undo.as_ref(), vim_normal.undo.as_slice()),
        (keymap.vim_normal.redo.as_ref(), vim_normal.redo.as_slice()),
        (
            keymap.vim_normal.cancel_operator.as_ref(),
            vim_normal.cancel_operator.as_slice(),
        ),
    ]);

    if keymap.vim_normal.start_change_operator.is_none() {
        vim_normal
            .start_change_operator
            .retain(|binding| !configured_vim_normal_bindings_to_preserve.contains(binding));
    }
    if keymap.vim_normal.substitute_char.is_none() {
        vim_normal
            .substitute_char
            .retain(|binding| !configured_vim_normal_bindings_to_preserve.contains(binding));
    }
    if keymap.vim_normal.replace_char.is_none() {
        vim_normal.replace_char.retain(|binding| {
            !configured_vim_normal_bindings_to_preserve.contains(binding)
                && !chords.bindings.iter().any(|chord| {
                    chord.action.context == KeymapContext::VimNormal
                        && chord.chord.prefix.parts() == binding.parts()
                })
        });
    }
    if keymap.vim_normal.repeat_last_change.is_none() {
        vim_normal.repeat_last_change.retain(|binding| {
            !configured_vim_normal_bindings_to_preserve.contains(binding)
                && !chords.bindings.iter().any(|chord| {
                    chord.action.context == KeymapContext::VimNormal
                        && chord.chord.prefix.parts() == binding.parts()
                })
        });
    }
    for (configured, bindings) in [
        (
            keymap.vim_normal.find_forward.as_ref(),
            &mut vim_normal.find_forward,
        ),
        (
            keymap.vim_normal.find_backward.as_ref(),
            &mut vim_normal.find_backward,
        ),
        (
            keymap.vim_normal.till_forward.as_ref(),
            &mut vim_normal.till_forward,
        ),
        (
            keymap.vim_normal.till_backward.as_ref(),
            &mut vim_normal.till_backward,
        ),
        (
            keymap.vim_normal.jump_top.as_ref(),
            &mut vim_normal.jump_top,
        ),
        (
            keymap.vim_normal.jump_bottom.as_ref(),
            &mut vim_normal.jump_bottom,
        ),
    ] {
        if configured.is_none() {
            bindings.retain(|binding| {
                !configured_vim_normal_bindings_to_preserve.contains(binding)
                    && !chords.bindings.iter().any(|chord| {
                        chord.action.context == KeymapContext::VimNormal
                            && chord.chord.prefix.parts() == binding.parts()
                    })
            });
        }
    }
    let mut vim_operator = VimOperatorKeymap {
        delete_line: resolve_local!(keymap, defaults, vim_operator, delete_line),
        yank_line: resolve_local!(keymap, defaults, vim_operator, yank_line),
        motion_left: resolve_local!(keymap, defaults, vim_operator, motion_left),
        motion_right: resolve_local!(keymap, defaults, vim_operator, motion_right),
        motion_up: resolve_local!(keymap, defaults, vim_operator, motion_up),
        motion_down: resolve_local!(keymap, defaults, vim_operator, motion_down),
        motion_word_forward: resolve_local!(keymap, defaults, vim_operator, motion_word_forward),
        motion_word_backward: resolve_local!(keymap, defaults, vim_operator, motion_word_backward),
        motion_word_end: resolve_local!(keymap, defaults, vim_operator, motion_word_end),
        motion_line_start: resolve_local!(keymap, defaults, vim_operator, motion_line_start),
        motion_line_end: resolve_local!(keymap, defaults, vim_operator, motion_line_end),
        motion_find_forward: resolve_local!(keymap, defaults, vim_operator, motion_find_forward),
        motion_find_backward: resolve_local!(keymap, defaults, vim_operator, motion_find_backward),
        motion_till_forward: resolve_local!(keymap, defaults, vim_operator, motion_till_forward),
        motion_till_backward: resolve_local!(keymap, defaults, vim_operator, motion_till_backward),
        motion_jump_top: resolve_local!(keymap, defaults, vim_operator, motion_jump_top),
        motion_jump_bottom: resolve_local!(keymap, defaults, vim_operator, motion_jump_bottom),
        select_inner_text_object: resolve_local!(
            keymap,
            defaults,
            vim_operator,
            select_inner_text_object
        ),
        select_around_text_object: resolve_local!(
            keymap,
            defaults,
            vim_operator,
            select_around_text_object
        ),
        cancel: resolve_local!(keymap, defaults, vim_operator, cancel),
    };

    let configured_vim_operator_bindings_to_preserve = configured_bindings_to_preserve([
        (
            keymap.vim_operator.delete_line.as_ref(),
            vim_operator.delete_line.as_slice(),
        ),
        (
            keymap.vim_operator.yank_line.as_ref(),
            vim_operator.yank_line.as_slice(),
        ),
        (
            keymap.vim_operator.motion_left.as_ref(),
            vim_operator.motion_left.as_slice(),
        ),
        (
            keymap.vim_operator.motion_right.as_ref(),
            vim_operator.motion_right.as_slice(),
        ),
        (
            keymap.vim_operator.motion_up.as_ref(),
            vim_operator.motion_up.as_slice(),
        ),
        (
            keymap.vim_operator.motion_down.as_ref(),
            vim_operator.motion_down.as_slice(),
        ),
        (
            keymap.vim_operator.motion_word_forward.as_ref(),
            vim_operator.motion_word_forward.as_slice(),
        ),
        (
            keymap.vim_operator.motion_word_backward.as_ref(),
            vim_operator.motion_word_backward.as_slice(),
        ),
        (
            keymap.vim_operator.motion_word_end.as_ref(),
            vim_operator.motion_word_end.as_slice(),
        ),
        (
            keymap.vim_operator.motion_line_start.as_ref(),
            vim_operator.motion_line_start.as_slice(),
        ),
        (
            keymap.vim_operator.motion_line_end.as_ref(),
            vim_operator.motion_line_end.as_slice(),
        ),
        (
            keymap.vim_operator.motion_find_forward.as_ref(),
            vim_operator.motion_find_forward.as_slice(),
        ),
        (
            keymap.vim_operator.motion_find_backward.as_ref(),
            vim_operator.motion_find_backward.as_slice(),
        ),
        (
            keymap.vim_operator.motion_till_forward.as_ref(),
            vim_operator.motion_till_forward.as_slice(),
        ),
        (
            keymap.vim_operator.motion_till_backward.as_ref(),
            vim_operator.motion_till_backward.as_slice(),
        ),
        (
            keymap.vim_operator.motion_jump_top.as_ref(),
            vim_operator.motion_jump_top.as_slice(),
        ),
        (
            keymap.vim_operator.motion_jump_bottom.as_ref(),
            vim_operator.motion_jump_bottom.as_slice(),
        ),
        (
            keymap.vim_operator.cancel.as_ref(),
            vim_operator.cancel.as_slice(),
        ),
    ]);

    if keymap.vim_operator.select_inner_text_object.is_none() {
        vim_operator
            .select_inner_text_object
            .retain(|binding| !configured_vim_operator_bindings_to_preserve.contains(binding));
    }
    if keymap.vim_operator.select_around_text_object.is_none() {
        vim_operator
            .select_around_text_object
            .retain(|binding| !configured_vim_operator_bindings_to_preserve.contains(binding));
    }
    for (configured, bindings) in [
        (
            keymap.vim_operator.motion_find_forward.as_ref(),
            &mut vim_operator.motion_find_forward,
        ),
        (
            keymap.vim_operator.motion_find_backward.as_ref(),
            &mut vim_operator.motion_find_backward,
        ),
        (
            keymap.vim_operator.motion_till_forward.as_ref(),
            &mut vim_operator.motion_till_forward,
        ),
        (
            keymap.vim_operator.motion_till_backward.as_ref(),
            &mut vim_operator.motion_till_backward,
        ),
        (
            keymap.vim_operator.motion_jump_top.as_ref(),
            &mut vim_operator.motion_jump_top,
        ),
        (
            keymap.vim_operator.motion_jump_bottom.as_ref(),
            &mut vim_operator.motion_jump_bottom,
        ),
    ] {
        if configured.is_none() {
            bindings.retain(|binding| {
                !configured_vim_operator_bindings_to_preserve.contains(binding)
                    && !chords.bindings.iter().any(|chord| {
                        chord.action.context == KeymapContext::VimOperator
                            && chord.chord.prefix.parts() == binding.parts()
                    })
            });
        }
    }

    let vim_text_object = VimTextObjectKeymap {
        word: resolve_local!(keymap, defaults, vim_text_object, word),
        big_word: resolve_local!(keymap, defaults, vim_text_object, big_word),
        parentheses: resolve_local!(keymap, defaults, vim_text_object, parentheses),
        brackets: resolve_local!(keymap, defaults, vim_text_object, brackets),
        braces: resolve_local!(keymap, defaults, vim_text_object, braces),
        double_quote: resolve_local!(keymap, defaults, vim_text_object, double_quote),
        single_quote: resolve_local!(keymap, defaults, vim_text_object, single_quote),
        backtick: resolve_local!(keymap, defaults, vim_text_object, backtick),
        cancel: resolve_local!(keymap, defaults, vim_text_object, cancel),
    };

    // New question shortcuts yield individually to existing explicit bindings.
    for (configured, bindings, aliases) in [
        (
            keymap.chat.prompt_stack_back.as_ref(),
            &mut chat.prompt_stack_back,
            vec![
                ("alt-down", key_hint::alt(KeyCode::Down)),
                ("shift-right", key_hint::shift(KeyCode::Right)),
            ],
        ),
        (
            keymap.chat.skip_question.as_ref(),
            &mut chat.skip_question,
            vec![("ctrl-]", key_hint::ctrl(KeyCode::Char(']')))],
        ),
    ] {
        if configured.is_none() {
            bindings.retain(|binding| {
                !aliases.iter().any(|(alias, candidate)| {
                    binding == candidate
                        && (configured_main_surface_alias_is_used(keymap, alias)
                            || configured_context_alias_is_used(&keymap.list, alias)
                            || configured_context_alias_is_used(&keymap.vim_search, alias))
                }) && !chords.bindings.iter().any(|chord| {
                    chord.action.context.overlaps(KeymapContext::Chat)
                        && binding.normalized_parts() == chord.chord.prefix.normalized_parts()
                })
            });
        }
    }

    // Reasoning arrow aliases are fallback defaults: existing explicit
    // bindings on the same input path keep the keys, while explicit
    // reasoning bindings remain authoritative.
    if keymap.chat.decrease_reasoning_effort.is_none()
        && configured_main_surface_alias_is_used(keymap, "shift-down")
    {
        chat.decrease_reasoning_effort
            .retain(|binding| *binding != key_hint::shift(KeyCode::Down));
    }
    if keymap.chat.increase_reasoning_effort.is_none()
        && configured_main_surface_alias_is_used(keymap, "shift-up")
    {
        chat.increase_reasoning_effort
            .retain(|binding| *binding != key_hint::shift(KeyCode::Up));
    }

    Ok((vim_normal, vim_operator, vim_text_object))
}
