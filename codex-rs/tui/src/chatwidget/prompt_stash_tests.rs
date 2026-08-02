use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

use super::*;
use crate::chatwidget::tests::make_chatwidget_manual_with_sender;

fn ctrl_s() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
}

#[tokio::test]
async fn ctrl_s_stashes_and_appends_the_prompt_on_restore() {
    let (mut chat, _sender, _rx, _op_rx) = make_chatwidget_manual_with_sender().await;
    chat.bottom_pane
        .set_composer_text("stashed prompt".to_string(), Vec::new(), Vec::new());

    chat.handle_key_event(ctrl_s());

    assert_eq!("", chat.bottom_pane.composer_text());
    assert_eq!(
        Some("stashed prompt"),
        chat.stashed_composer
            .as_ref()
            .map(|composer| composer.text.as_str())
    );

    chat.bottom_pane
        .set_composer_text("current prompt: ".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(ctrl_s());

    assert_eq!(
        "current prompt: stashed prompt",
        chat.bottom_pane.composer_text()
    );
    assert_eq!(None, chat.stashed_composer);
}

#[tokio::test]
async fn restore_remaps_colliding_large_paste_placeholders() {
    let (mut chat, _sender, _rx, _op_rx) = make_chatwidget_manual_with_sender().await;
    let placeholder = "[Pasted Content 5 chars]";
    chat.restore_composer_state(ThreadComposerState {
        text: placeholder.to_string(),
        text_elements: vec![TextElement::new(
            (0..placeholder.len()).into(),
            Some(placeholder.to_string()),
        )],
        pending_pastes: vec![(placeholder.to_string(), "alpha".to_string())],
        ..ThreadComposerState::default()
    });
    chat.handle_key_event(ctrl_s());

    chat.restore_composer_state(ThreadComposerState {
        text: placeholder.to_string(),
        text_elements: vec![TextElement::new(
            (0..placeholder.len()).into(),
            Some(placeholder.to_string()),
        )],
        pending_pastes: vec![(placeholder.to_string(), "bravo".to_string())],
        ..ThreadComposerState::default()
    });
    chat.handle_key_event(ctrl_s());

    assert_eq!(
        format!("{placeholder}{placeholder} #2"),
        chat.bottom_pane.composer_text()
    );
    assert_eq!(
        vec![
            (placeholder.to_string(), "bravo".to_string()),
            (format!("{placeholder} #2"), "alpha".to_string()),
        ],
        chat.bottom_pane.composer_pending_pastes()
    );
    assert_eq!(None, chat.stashed_composer);
}

#[tokio::test]
async fn ctrl_s_keeps_history_search_ownership() {
    let (mut chat, _sender, _rx, _op_rx) = make_chatwidget_manual_with_sender().await;
    chat.bottom_pane
        .set_composer_text("search draft".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));

    assert!(chat.bottom_pane.composer_history_search_active());
    chat.handle_key_event(ctrl_s());

    assert!(chat.bottom_pane.composer_history_search_active());
    assert_eq!(None, chat.stashed_composer);
}
