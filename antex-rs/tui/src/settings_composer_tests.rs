use super::*;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

#[test]
pub(super) fn configured_vim_start_and_russian_commands_reach_the_composer() {
    let settings = Settings::from_config(&serde_json::json!({"vim_mode_default":true,"vim_mode_start":"insert","show_vim_mode_indicator":true}), "/home/test/.antex".into()).unwrap();
    let mut composer = crate::composer::Composer::new(settings.keymap.clone());
    composer.configure(&settings);
    composer.paste("привет").unwrap();
    assert_eq!(composer.mode_label(), Some("Insert"));
    composer
        .key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(composer.mode_label(), Some("Normal"));
    composer
        .key(KeyEvent::new(KeyCode::Char('ш'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(composer.mode_label(), Some("Insert"));
    assert_eq!(
        composer.draft().input(),
        antex_core::UserInput::from("привет")
    );
}

#[test]
pub(super) fn configured_submit_chord_preserves_the_draft_and_dispatches_once() {
    let settings = Settings::from_config(
        &serde_json::json!({"keymap":{"composer":{"submit":"f20 f21"}}}),
        "/home/test/.antex".into(),
    )
    .unwrap();
    let mut composer = crate::composer::Composer::new(settings.keymap.clone());
    composer.configure(&settings);
    composer.paste("keep").unwrap();
    assert_eq!(
        composer
            .key(KeyEvent::new(KeyCode::F(20), KeyModifiers::NONE))
            .unwrap(),
        None
    );
    assert_eq!(
        composer
            .key(KeyEvent::new(KeyCode::F(21), KeyModifiers::NONE))
            .unwrap(),
        Some(crate::composer::ComposerAction::Submit(
            crate::composer::SubmitMode::Send
        ))
    );
    assert_eq!(
        composer.draft().input(),
        antex_core::UserInput::from("keep")
    );
}
