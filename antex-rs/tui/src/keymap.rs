//! Runtime keymap resolution for the TUI.
//!
//! This module converts deserialized config (`TuiKeymap`) into a concrete
//! `RuntimeKeymap` used by input handlers at runtime.
//!
//! Key responsibilities:
//!
//! 1. Apply deterministic precedence (`context -> global fallback -> defaults`).
//! 2. Parse canonical key spec strings into `KeyBinding` values.
//! 3. Enforce uniqueness across runtime surfaces so one key cannot trigger
//!    multiple actions on the same focused input path.
//! 4. Return actionable, user-facing error messages with config paths and next
//!    steps.
//!
//! Non-responsibilities:
//!
//! 1. This module does not decide which action should run in a given screen.
//!    Callers resolve actions by checking the relevant action binding set.
//! 2. This module does not persist configuration; it only resolves loaded config.

use crate::key_hint;
use crate::key_hint::KeyBinding;
use crate::key_hint::KeyBindingListExt;
use crate::key_hint::ShortcutHint;
use crate::keymap_config::KeybindingsSpec;
use crate::keymap_config::MAX_FUNCTION_KEY;
use crate::keymap_config::TuiKeymap;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

#[path = "keymap/bindings.rs"]
mod bindings;
#[path = "keymap/chords.rs"]
mod chords;
#[path = "keymap/vim_search.rs"]
mod vim_search;
pub(crate) use vim_search::VimSearchKeymap;

#[cfg(test)]
#[path = "keymap/conflict_tests.rs"]
mod conflict_tests;

pub(crate) use bindings::KeymapContext;
pub(crate) use bindings::bindings_for_action;
pub(crate) use bindings::keymap_action_id;
use bindings::runtime_action_bindings;
pub(crate) use chords::KeyChordMatch;
pub(crate) use chords::KeyChordMatcher;
pub(crate) use chords::KeymapContextSet;
pub(crate) use chords::RuntimeChordKeymap;

/// Runtime keymap used by TUI input handlers.
///
/// Resolution precedence is:
///
/// 1. Context-specific binding (`tui.keymap.<context>`).
/// 2. `tui.keymap.global` for actions that support global fallback.
/// 3. Built-in defaults.
///
/// This is the only shape UI code should use for dispatch. It represents a
/// fully resolved snapshot with parsing, fallback, explicit unbinding, and
/// duplicate-key validation already applied. If a caller keeps using an older
/// snapshot after config changes, visible hints and active handlers can drift.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeKeymap {
    pub(crate) app: AppKeymap,
    pub(crate) chords: Arc<RuntimeChordKeymap>,
    pub(crate) chat: ChatKeymap,
    pub(crate) composer: ComposerKeymap,
    pub(crate) editor: Arc<EditorKeymap>,
    pub(crate) vim_normal: VimNormalKeymap,
    pub(crate) vim_operator: VimOperatorKeymap,
    pub(crate) vim_search: VimSearchKeymap,
    pub(crate) vim_text_object: VimTextObjectKeymap,
    pub(crate) pager: PagerKeymap,
    pub(crate) list: ListKeymap,
    pub(crate) agents: AgentsKeymap,
    pub(crate) approval: ApprovalKeymap,
}

#[derive(Clone, Debug)]
pub(crate) struct AppKeymap {
    /// Open the daemon-wide agent-session overview.
    pub(crate) open_agents: Vec<KeyBinding>,
    /// Open transcript overlay.
    pub(crate) open_transcript: Vec<KeyBinding>,
    /// Open external editor for the current draft.
    pub(crate) open_external_editor: Vec<KeyBinding>,
    /// Copy the last agent response to the clipboard.
    pub(crate) copy: Vec<KeyBinding>,
    /// Clear the terminal UI.
    pub(crate) clear_terminal: Vec<KeyBinding>,
    /// Toggle Vim mode for the composer input.
    pub(crate) toggle_vim_mode: Vec<KeyBinding>,
    /// Toggle Fast mode.
    pub(crate) toggle_fast_mode: Vec<KeyBinding>,
    /// Toggle raw scrollback mode for copy-friendly transcript selection.
    pub(crate) toggle_raw_output: Vec<KeyBinding>,
    /// Switch between a side conversation and its parent without closing either.
    pub(crate) toggle_side_conversation: Vec<KeyBinding>,
}

/// Chat-level keybindings evaluated at the app event layer.
///
/// These participate in the first app-scope conflict validation pass alongside
/// `AppKeymap` actions because both are checked before input reaches the
/// composer. Dispatch gating (empty-composer guard for backtrack) happens in
/// handler code, not here.
#[derive(Clone, Debug)]
pub(crate) struct ChatKeymap {
    /// Interrupt the active turn.
    pub(crate) interrupt_turn: Vec<KeyBinding>,
    /// Decrease the active reasoning effort.
    pub(crate) decrease_reasoning_effort: Vec<KeyBinding>,
    /// Increase the active reasoning effort.
    pub(crate) increase_reasoning_effort: Vec<KeyBinding>,
    /// Switch to the previous available permission mode.
    pub(crate) previous_permission_mode: Vec<KeyBinding>,
    /// Switch to the next available permission mode.
    pub(crate) next_permission_mode: Vec<KeyBinding>,
    /// Move up through async questions, then edit the most recently queued message.
    pub(crate) edit_queued_message: Vec<KeyBinding>,
    /// Move back through async questions toward the composer.
    pub(crate) prompt_stack_back: Vec<KeyBinding>,
    /// Skip the focused question.
    pub(crate) skip_question: Vec<KeyBinding>,
}

/// Composer-level keybindings validated in the second app-scope conflict pass.
///
/// App-level handlers execute before the composer receives input, so any key
/// bound here that also appears in `AppKeymap` would be silently intercepted.
/// The conflict validator prevents this by checking app + composer uniqueness.
#[derive(Clone, Debug)]
pub(crate) struct ComposerKeymap {
    /// Submit current draft.
    pub(crate) submit: Vec<KeyBinding>,
    /// Queue current draft while a task is running.
    pub(crate) queue: Vec<KeyBinding>,
    /// Toggle composer shortcut overlay.
    pub(crate) toggle_shortcuts: Vec<KeyBinding>,
    /// Open reverse history search or move to the previous match.
    pub(crate) history_search_previous: Vec<KeyBinding>,
    /// Move to the next match in reverse history search.
    pub(crate) history_search_next: Vec<KeyBinding>,
}

/// Editor-specific keybindings used by the composer textarea.
///
/// These bindings are interpreted only by text-editing widgets and do not
/// participate in global/chat fallback resolution.
#[derive(Clone, Debug)]
pub(crate) struct EditorKeymap {
    pub(crate) insert_newline: Vec<KeyBinding>,
    pub(crate) move_left: Vec<KeyBinding>,
    pub(crate) move_right: Vec<KeyBinding>,
    pub(crate) move_up: Vec<KeyBinding>,
    pub(crate) move_down: Vec<KeyBinding>,
    pub(crate) move_word_left: Vec<KeyBinding>,
    pub(crate) move_word_right: Vec<KeyBinding>,
    pub(crate) move_line_start: Vec<KeyBinding>,
    pub(crate) move_line_end: Vec<KeyBinding>,
    pub(crate) delete_backward: Vec<KeyBinding>,
    pub(crate) delete_forward: Vec<KeyBinding>,
    pub(crate) delete_backward_word: Vec<KeyBinding>,
    pub(crate) delete_forward_word: Vec<KeyBinding>,
    pub(crate) kill_line_start: Vec<KeyBinding>,
    pub(crate) kill_whole_line: Vec<KeyBinding>,
    pub(crate) kill_line_end: Vec<KeyBinding>,
    pub(crate) yank: Vec<KeyBinding>,
}

/// Vim normal-mode keybindings for modal editing in the composer textarea.
///
/// Normal mode is the resting state when Vim is enabled. Pressing a movement
/// or editing key here either moves the cursor, triggers an operator-pending
/// state (via `start_delete_operator`, `start_yank_operator`, or `start_change_operator`), or transitions
/// to insert mode. Default bindings include both `shift(letter)` and
/// `plain(UPPERCASE)` variants for uppercase commands like `A`, `I`, `O` to
/// handle cross-terminal shift-reporting inconsistencies.
#[derive(Clone, Debug, Default)]
pub(crate) struct VimNormalKeymap {
    pub(crate) enter_insert: Vec<KeyBinding>,
    pub(crate) append_after_cursor: Vec<KeyBinding>,
    pub(crate) append_line_end: Vec<KeyBinding>,
    pub(crate) insert_line_start: Vec<KeyBinding>,
    pub(crate) open_line_below: Vec<KeyBinding>,
    pub(crate) open_line_above: Vec<KeyBinding>,
    pub(crate) enter_replace_mode: Vec<KeyBinding>,
    pub(crate) move_left: Vec<KeyBinding>,
    pub(crate) move_right: Vec<KeyBinding>,
    pub(crate) move_up: Vec<KeyBinding>,
    pub(crate) move_down: Vec<KeyBinding>,
    pub(crate) move_word_forward: Vec<KeyBinding>,
    pub(crate) move_word_backward: Vec<KeyBinding>,
    pub(crate) move_word_end: Vec<KeyBinding>,
    pub(crate) move_line_start: Vec<KeyBinding>,
    pub(crate) move_line_end: Vec<KeyBinding>,
    pub(crate) find_forward: Vec<KeyBinding>,
    pub(crate) find_backward: Vec<KeyBinding>,
    pub(crate) till_forward: Vec<KeyBinding>,
    pub(crate) till_backward: Vec<KeyBinding>,
    pub(crate) jump_top: Vec<KeyBinding>,
    pub(crate) jump_bottom: Vec<KeyBinding>,
    pub(crate) delete_char: Vec<KeyBinding>,
    pub(crate) replace_char: Vec<KeyBinding>,
    pub(crate) repeat_last_change: Vec<KeyBinding>,
    pub(crate) substitute_char: Vec<KeyBinding>,
    pub(crate) delete_to_line_end: Vec<KeyBinding>,
    pub(crate) change_to_line_end: Vec<KeyBinding>,
    pub(crate) yank_line: Vec<KeyBinding>,
    pub(crate) paste_after: Vec<KeyBinding>,
    pub(crate) start_delete_operator: Vec<KeyBinding>,
    pub(crate) start_yank_operator: Vec<KeyBinding>,
    pub(crate) start_change_operator: Vec<KeyBinding>,
    pub(crate) undo: Vec<KeyBinding>,
    pub(crate) redo: Vec<KeyBinding>,
    pub(crate) cancel_operator: Vec<KeyBinding>,
}

/// Vim operator-pending keybindings active after `d`, `y`, or `c` in normal mode.
///
/// When a delete, yank, or change operator is
/// pressed, the next keypress is matched against this context to determine the
/// motion range. Repeating the operator key (`dd`, `yy`) acts on the whole
/// line. `Esc` cancels the pending operator and returns to normal mode.
#[derive(Clone, Debug, Default)]
pub(crate) struct VimOperatorKeymap {
    pub(crate) delete_line: Vec<KeyBinding>,
    pub(crate) yank_line: Vec<KeyBinding>,
    pub(crate) motion_left: Vec<KeyBinding>,
    pub(crate) motion_right: Vec<KeyBinding>,
    pub(crate) motion_up: Vec<KeyBinding>,
    pub(crate) motion_down: Vec<KeyBinding>,
    pub(crate) motion_word_forward: Vec<KeyBinding>,
    pub(crate) motion_word_backward: Vec<KeyBinding>,
    pub(crate) motion_word_end: Vec<KeyBinding>,
    pub(crate) motion_line_start: Vec<KeyBinding>,
    pub(crate) motion_line_end: Vec<KeyBinding>,
    pub(crate) motion_find_forward: Vec<KeyBinding>,
    pub(crate) motion_find_backward: Vec<KeyBinding>,
    pub(crate) motion_till_forward: Vec<KeyBinding>,
    pub(crate) motion_till_backward: Vec<KeyBinding>,
    pub(crate) motion_jump_top: Vec<KeyBinding>,
    pub(crate) motion_jump_bottom: Vec<KeyBinding>,
    pub(crate) select_inner_text_object: Vec<KeyBinding>,
    pub(crate) select_around_text_object: Vec<KeyBinding>,
    pub(crate) cancel: Vec<KeyBinding>,
}

/// Vim text-object keybindings active after an operator plus inner/around prefix.
#[derive(Clone, Debug, Default)]
pub(crate) struct VimTextObjectKeymap {
    pub(crate) word: Vec<KeyBinding>,
    pub(crate) big_word: Vec<KeyBinding>,
    pub(crate) parentheses: Vec<KeyBinding>,
    pub(crate) brackets: Vec<KeyBinding>,
    pub(crate) braces: Vec<KeyBinding>,
    pub(crate) double_quote: Vec<KeyBinding>,
    pub(crate) single_quote: Vec<KeyBinding>,
    pub(crate) backtick: Vec<KeyBinding>,
    pub(crate) cancel: Vec<KeyBinding>,
}

/// Pager/overlay keybindings for transcript and static help views.
#[derive(Clone, Debug)]
pub(crate) struct PagerKeymap {
    pub(crate) scroll_up: Vec<KeyBinding>,
    pub(crate) scroll_down: Vec<KeyBinding>,
    pub(crate) page_up: Vec<KeyBinding>,
    pub(crate) page_down: Vec<KeyBinding>,
    pub(crate) half_page_up: Vec<KeyBinding>,
    pub(crate) half_page_down: Vec<KeyBinding>,
    pub(crate) jump_top: Vec<KeyBinding>,
    pub(crate) jump_bottom: Vec<KeyBinding>,
    pub(crate) close: Vec<KeyBinding>,
    pub(crate) close_transcript: Vec<KeyBinding>,
    chord_hints: Arc<RuntimeChordKeymap>,
}

impl PagerKeymap {
    pub(crate) fn primary_hint(
        &self,
        action: &'static str,
        bindings: &[KeyBinding],
    ) -> Option<ShortcutHint> {
        let action = keymap_action_id(KeymapContext::Pager.config_name(), action)?;
        self.chord_hints.primary_hint(action, bindings)
    }
}

/// Semantic navigation and confirmation actions shared by list-like views.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ListAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PageUp,
    PageDown,
    JumpTop,
    JumpBottom,
    Accept,
    Cancel,
}

impl ListAction {
    fn config_name(self) -> &'static str {
        match self {
            Self::MoveUp => "move_up",
            Self::MoveDown => "move_down",
            Self::MoveLeft => "move_left",
            Self::MoveRight => "move_right",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::JumpTop => "jump_top",
            Self::JumpBottom => "jump_bottom",
            Self::Accept => "accept",
            Self::Cancel => "cancel",
        }
    }
}

/// Generic list picker keybindings shared across popup list views.
///
/// These actions describe list intent rather than a specific widget layout.
/// Vertical actions move the highlighted row, page and jump actions move within
/// the current filtered row set, and horizontal actions are available to views
/// that expose adjacent choices such as tabs, toolbar values, or ordered item
/// movement. Views that also accept search text are responsible for checking
/// `is_plain_text_key_event` before dispatching plain-character bindings so a
/// configured `j`, `k`, `h`, or `l` does not steal query input.
#[derive(Clone, Debug)]
pub(crate) struct ListKeymap {
    pub(crate) move_up: Vec<KeyBinding>,
    pub(crate) move_down: Vec<KeyBinding>,
    pub(crate) move_left: Vec<KeyBinding>,
    pub(crate) move_right: Vec<KeyBinding>,
    pub(crate) page_up: Vec<KeyBinding>,
    pub(crate) page_down: Vec<KeyBinding>,
    pub(crate) jump_top: Vec<KeyBinding>,
    pub(crate) jump_bottom: Vec<KeyBinding>,
    pub(crate) accept: Vec<KeyBinding>,
    pub(crate) cancel: Vec<KeyBinding>,
    chord_hints: Arc<RuntimeChordKeymap>,
}

impl ListKeymap {
    pub(crate) fn bindings_for(&self, action: ListAction) -> &[KeyBinding] {
        match action {
            ListAction::MoveUp => &self.move_up,
            ListAction::MoveDown => &self.move_down,
            ListAction::MoveLeft => &self.move_left,
            ListAction::MoveRight => &self.move_right,
            ListAction::PageUp => &self.page_up,
            ListAction::PageDown => &self.page_down,
            ListAction::JumpTop => &self.jump_top,
            ListAction::JumpBottom => &self.jump_bottom,
            ListAction::Accept => &self.accept,
            ListAction::Cancel => &self.cancel,
        }
    }

    pub(crate) fn action_for(&self, event: KeyEvent) -> Option<ListAction> {
        [
            ListAction::MoveUp,
            ListAction::MoveDown,
            ListAction::MoveLeft,
            ListAction::MoveRight,
            ListAction::PageUp,
            ListAction::PageDown,
            ListAction::JumpTop,
            ListAction::JumpBottom,
            ListAction::Accept,
            ListAction::Cancel,
        ]
        .into_iter()
        .find(|action| self.bindings_for(*action).is_pressed(event))
    }

    pub(crate) fn primary_hint(&self, action: ListAction) -> Option<ShortcutHint> {
        let action_id = keymap_action_id(KeymapContext::List.config_name(), action.config_name())?;
        self.chord_hints
            .primary_hint(action_id, self.bindings_for(action))
    }
}

/// Task-management shortcuts specific to the shared agents dashboard.
#[derive(Clone, Debug)]
pub(crate) struct AgentsKeymap {
    pub(crate) resume: Vec<KeyBinding>,
    pub(crate) search: Vec<KeyBinding>,
    pub(crate) new_task: Vec<KeyBinding>,
    pub(crate) rename: Vec<KeyBinding>,
    pub(crate) stop: Vec<KeyBinding>,
    pub(crate) toggle_grouping: Vec<KeyBinding>,
    chord_hints: Arc<RuntimeChordKeymap>,
}

impl AgentsKeymap {
    pub(crate) fn primary_hint(
        &self,
        action: &'static str,
        bindings: &[KeyBinding],
    ) -> Option<ShortcutHint> {
        let action_id = keymap_action_id(KeymapContext::Agents.config_name(), action)?;
        self.chord_hints.primary_hint(action_id, bindings)
    }
}

/// Approval modal keybindings.
///
/// This covers both selection actions and the "open details fullscreen" escape
/// hatch for large approval payloads.
#[derive(Clone, Debug)]
pub(crate) struct ApprovalKeymap {
    pub(crate) open_fullscreen: Vec<KeyBinding>,
    pub(crate) open_thread: Vec<KeyBinding>,
    pub(crate) approve: Vec<KeyBinding>,
    pub(crate) approve_for_session: Vec<KeyBinding>,
    pub(crate) approve_for_prefix: Vec<KeyBinding>,
    pub(crate) deny: Vec<KeyBinding>,
    pub(crate) decline: Vec<KeyBinding>,
    pub(crate) cancel: Vec<KeyBinding>,
    chord_hints: Arc<RuntimeChordKeymap>,
}

impl ApprovalKeymap {
    pub(crate) fn primary_hint(
        &self,
        action: &'static str,
        bindings: &[KeyBinding],
    ) -> Option<ShortcutHint> {
        let action = keymap_action_id(KeymapContext::Approval.config_name(), action)?;
        self.chord_hints.primary_hint(action, bindings)
    }

    pub(crate) fn hint_for_bindings(&self, bindings: &[KeyBinding]) -> Option<ShortcutHint> {
        [
            ("open_fullscreen", self.open_fullscreen.as_slice()),
            ("open_thread", self.open_thread.as_slice()),
            ("approve", self.approve.as_slice()),
            ("approve_for_session", self.approve_for_session.as_slice()),
            ("approve_for_prefix", self.approve_for_prefix.as_slice()),
            ("deny", self.deny.as_slice()),
            ("decline", self.decline.as_slice()),
            ("cancel", self.cancel.as_slice()),
        ]
        .into_iter()
        .find_map(|(action, configured)| {
            (!bindings.is_empty() && bindings.iter().all(|binding| configured.contains(binding)))
                .then(|| self.primary_hint(action, configured))
                .flatten()
        })
        .or_else(|| primary_binding(bindings).map(ShortcutHint::from))
    }
}

/// Returns the first binding, used as the primary UI hint for an action.
///
/// Rendering code should prefer this for concise hints while preserving all
/// bindings for actual input matching.
pub(crate) fn primary_binding(bindings: &[KeyBinding]) -> Option<KeyBinding> {
    user_bindings(bindings).first().copied()
}

/// Remove the trailing internal dispatch token, when present.
pub(crate) fn user_bindings(bindings: &[KeyBinding]) -> &[KeyBinding] {
    if bindings
        .last()
        .is_some_and(|binding| chords::is_dispatch_token(*binding))
    {
        &bindings[..bindings.len() - 1]
    } else {
        bindings
    }
}

/// Resolve one context-local action binding from config.
///
/// Expands to `resolve_bindings(...)` with:
/// - configured source: `tui.keymap.<context>.<action>`
/// - fallback source: the same action from built-in defaults
/// - error path: a stable string path for user-facing diagnostics
///
/// This keeps the resolution table concise while guaranteeing path strings
/// stay in sync with field names.
macro_rules! resolve_local {
    ($keymap:expr, $defaults:expr, $context:ident, $action:ident) => {
        resolve_bindings(
            ($keymap).$context.$action.as_ref(),
            &($defaults).$context.$action,
            concat!(
                "tui.keymap.",
                stringify!($context),
                ".",
                stringify!($action)
            ),
        )?
    };
}

/// Resolve one action binding with global fallback.
///
/// Expands to `resolve_bindings_with_global_fallback(...)` with precedence:
/// 1. `tui.keymap.<context>.<action>`
/// 2. `tui.keymap.global.<action>`
/// 3. built-in defaults for `<context>.<action>`
///
/// Used only for actions that intentionally support global reuse.
/// Context-local empty lists still count as configured values, so they unbind
/// the action instead of falling back to `global`.
macro_rules! resolve_with_global {
    ($keymap:expr, $defaults:expr, $context:ident, $action:ident) => {
        resolve_bindings_with_global_fallback(
            ($keymap).$context.$action.as_ref(),
            ($keymap).global.$action.as_ref(),
            &($defaults).$context.$action,
            concat!(
                "tui.keymap.",
                stringify!($context),
                ".",
                stringify!($action)
            ),
        )?
    };
}

/// Expand one default-table binding entry into a [`KeyBinding`].
///
/// This is a small declarative layer over `key_hint::{plain, ctrl, alt, shift}`
/// used by `default_bindings!` so `built_in_defaults` stays readable.
///
/// Supported forms:
/// - `plain(<KeyCode>)`
/// - `ctrl(<KeyCode>)`
/// - `alt(<KeyCode>)`
/// - `shift(<KeyCode>)`
/// - `raw(<KeyBinding expression>)` for bindings that do not match the helpers
///   (for example combined modifiers like Ctrl+Shift).
macro_rules! default_binding {
    (plain($key:expr)) => {
        key_hint::plain($key)
    };
    (ctrl($key:expr)) => {
        key_hint::ctrl($key)
    };
    (alt($key:expr)) => {
        key_hint::alt($key)
    };
    (shift($key:expr)) => {
        key_hint::shift($key)
    };
    (raw($binding:expr)) => {
        $binding
    };
}

/// Build a `Vec<KeyBinding>` for built-in defaults.
///
/// This macro is intentionally scoped to built-in keymaps. Runtime
/// config parsing still goes through `parse_bindings(...)` so user errors can
/// be reported with config-path-aware diagnostics.
macro_rules! default_bindings {
    ($($kind:ident($($arg:tt)*)),* $(,)?) => {
        vec![$(default_binding!($kind($($arg)*))),*]
    };
}

mod resolve_vim;

mod defaults;
mod resolve;
mod validation;

use resolve::configured_bindings_to_preserve;
use resolve::configured_context_alias_is_used;
use resolve::configured_main_surface_alias_is_used;
use resolve::parse_keybinding;
use resolve::resolve_bindings;
use validation::MAIN_RESERVED_BINDINGS;
use validation::TRANSCRIPT_BACKTRACK_RESERVED_BINDINGS;
use validation::validate_unique;

#[cfg(test)]
#[path = "keymap_tests.rs"]
mod tests;
