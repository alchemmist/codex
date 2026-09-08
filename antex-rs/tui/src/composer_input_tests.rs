use super::*;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;
use std::time::Duration;
use std::time::Instant;

#[test]
fn unbracketed_multiline_paste_never_submits_embedded_newlines() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    let now = Instant::now();
    let text = "echo first\necho second\n";
    for ch in text.chars() {
        let code = if ch == '\n' {
            KeyCode::Enter
        } else {
            KeyCode::Char(ch)
        };
        assert_eq!(
            composer
                .key_at(KeyEvent::new(code, KeyModifiers::NONE), now)
                .unwrap(),
            None
        );
    }
    assert!(composer.flush_paste(now + Duration::from_millis(20)));
    assert_eq!(composer.draft().input(), UserInput::from(text));
    assert_eq!(
        composer
            .key_at(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                now + Duration::from_millis(200)
            )
            .unwrap(),
        Some(ComposerAction::Submit(SubmitMode::Send))
    );
}

#[test]
fn modified_keys_flush_pending_text_before_moving_the_cursor() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    let now = Instant::now();
    composer
        .key_at(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), now)
        .unwrap();
    composer
        .key_at(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), now)
        .unwrap();
    assert_eq!((composer.editor.text(), composer.editor.cursor()), ("a", 0));
}

#[test]
fn stashing_includes_a_character_held_by_paste_detection() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    composer
        .key_at(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            Instant::now(),
        )
        .unwrap();
    composer.toggle_stash(|_| Ok(())).unwrap();
    composer.toggle_stash(|_| Ok(())).unwrap();
    assert_eq!(composer.draft().input(), UserInput::from("a"));
}
