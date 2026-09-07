use super::*;
use pretty_assertions::assert_eq;

#[test]
fn untrusted_terminal_sequences_are_visible_but_cannot_execute() {
    assert_eq!(
        safe_text("name\x1b]52;c;secret\x07\rnext"),
        "name\\u{1b}]52;c;secret\\u{7}\\rnext"
    );
    assert_eq!(safe_text("Привет 👩‍💻\n\ttext"), "Привет 👩‍💻\n\ttext");
}

#[test]
fn large_display_content_is_bounded_without_splitting_unicode() {
    let visible = safe_text(&"👩‍💻".repeat(100_000));
    assert!(visible.len() < 66 * 1024);
    assert!(visible.ends_with("[display truncated; full content remains in the session]"));
}
