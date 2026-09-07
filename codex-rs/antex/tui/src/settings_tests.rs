use super::*;
use pretty_assertions::assert_eq;



#[test]
fn settings_diagnostics_do_not_echo_unsupported_values() {
    let settings = Settings::from_config(
        &serde_json::json!({"removed_flag":"secret-value"}),
        "/home/test/.antex".into(),
    )
    .unwrap();
    assert_eq!(
        settings.warnings,
        vec!["unsupported TUI configuration key: removed_flag"]
    );
    let error = Settings::from_config(
        &serde_json::json!({"vim_mode_default":"secret-value"}),
        "/home/test/.antex".into(),
    )
    .err()
    .unwrap();
    assert!(!error.contains("secret-value"));
}

#[test]
fn fixed_stash_shortcut_cannot_be_shadowed_by_configured_actions() {
    assert!(
        Settings::from_config(
            &serde_json::json!({"keymap":{"composer":{"submit":"ctrl-s"}}}),
            "/home/test/.antex".into()
        )
        .is_err()
    );
}
