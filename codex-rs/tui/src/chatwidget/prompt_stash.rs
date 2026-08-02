//! Single-slot prompt stash behavior for the chat composer.

use std::collections::HashSet;

use super::user_messages::append_user_messages;
use super::user_messages::remap_colliding_paste_placeholders;
use super::*;

impl ChatWidget {
    pub(super) fn toggle_prompt_stash(&mut self) {
        if let Some(stashed_composer) = self.stashed_composer.take() {
            let draft = self.bottom_pane.composer_draft_snapshot();
            let current_composer = ThreadComposerState {
                text: draft.text,
                text_elements: draft.text_elements,
                local_images: draft.local_images,
                remote_image_urls: draft.remote_image_urls,
                mention_bindings: draft.mention_bindings,
                pending_pastes: draft.pending_pastes,
            };

            let restored_composer = if current_composer.has_content() {
                let mut used_paste_placeholders = HashSet::new();
                let current_message = UserMessage {
                    text: current_composer.text,
                    text_elements: current_composer.text_elements,
                    local_images: current_composer.local_images,
                    remote_image_urls: current_composer.remote_image_urls,
                    mention_bindings: current_composer.mention_bindings,
                };
                let stashed_message = UserMessage {
                    text: stashed_composer.text,
                    text_elements: stashed_composer.text_elements,
                    local_images: stashed_composer.local_images,
                    remote_image_urls: stashed_composer.remote_image_urls,
                    mention_bindings: stashed_composer.mention_bindings,
                };
                let (current_message, mut pending_pastes) = remap_colliding_paste_placeholders(
                    current_message,
                    current_composer.pending_pastes,
                    &mut used_paste_placeholders,
                );
                let (stashed_message, stashed_pending_pastes) = remap_colliding_paste_placeholders(
                    stashed_message,
                    stashed_composer.pending_pastes,
                    &mut used_paste_placeholders,
                );
                pending_pastes.extend(stashed_pending_pastes);
                Self::composer_state_from_user_message(
                    append_user_messages(vec![current_message, stashed_message]),
                    pending_pastes,
                )
            } else {
                stashed_composer
            };

            self.restore_composer_state(restored_composer);
            self.bottom_pane.hide_prompt_stashed_indicator();
            self.request_redraw();
            return;
        }

        let draft = self.bottom_pane.composer_draft_snapshot();
        let composer = ThreadComposerState {
            text: draft.text,
            text_elements: draft.text_elements,
            local_images: draft.local_images,
            remote_image_urls: draft.remote_image_urls,
            mention_bindings: draft.mention_bindings,
            pending_pastes: draft.pending_pastes,
        };
        if !composer.has_content() {
            return;
        }

        self.stashed_composer = Some(composer);
        self.restore_composer_state(ThreadComposerState::default());
        self.bottom_pane.show_prompt_stashed_indicator();
        self.request_redraw();
    }
}

#[cfg(test)]
#[path = "prompt_stash_tests.rs"]
mod tests;
