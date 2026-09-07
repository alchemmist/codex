use super::*;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

#[test]
fn vim_edit_transactions_restore_text_cursor_and_repeat_state() {
    let settings = crate::Settings::from_config(
        &serde_json::json!({"vim_mode_default":true}),
        "/home/test/.antex".into(),
    )
    .unwrap();
    for (original, command, expected) in [
        ("one two", "cwX", "X two"),
        ("abc", "iXYZ", "XYZabc"),
        ("abc-def", "df-", "def"),
    ] {
        let mut composer = Composer::new(settings.keymap.clone());
        composer.configure(&settings);
        composer.paste(original).unwrap();
        composer.editor.set_cursor(0);
        for ch in command.chars() {
            composer
                .key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
                .unwrap();
        }
        composer
            .key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(composer.editor.text(), expected);
        composer
            .key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(
            (composer.editor.text(), composer.editor.cursor()),
            (original, 0)
        );
        composer
            .key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(composer.editor.text(), expected);
    }
}

#[test]
fn vim_search_operator_confirms_instead_of_submitting_the_draft() {
    let settings = crate::Settings::from_config(
        &serde_json::json!({"vim_mode_default":true}),
        "/home/test/.antex".into(),
    )
    .unwrap();
    let mut composer = Composer::new(settings.keymap.clone());
    composer.configure(&settings);
    composer.paste("one two").unwrap();
    composer.editor.set_cursor(0);
    for ch in "d/two".chars() {
        composer
            .key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
            .unwrap();
    }
    assert_eq!(
        composer
            .key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap(),
        None
    );
    assert_eq!(composer.editor.text(), "two");
    composer
        .key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(composer.editor.text(), "one two");
}

#[test]
fn vim_undo_restores_image_attachments_as_part_of_the_draft() {
    let settings = crate::Settings::from_config(
        &serde_json::json!({"vim_mode_default":true}),
        "/home/test/.antex".into(),
    )
    .unwrap();
    let mut composer = Composer::new(settings.keymap.clone());
    composer.configure(&settings);
    composer
        .attach("image/png".into(), Arc::from([1, 2, 3]))
        .unwrap();
    let expected = composer.draft();
    composer.editor.set_cursor(0);
    composer
        .key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    assert!(composer.draft().images.is_empty());
    composer
        .key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(composer.draft(), expected);
}
