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

#[test]
fn edit_preview_keeps_deletions_insertions_and_the_target_visible() {
    let call = antex_core::ToolCall {
        id: "edit-1".into(),
        name: "edit".into(),
        arguments: serde_json::json!({"path":"src/main.rs","old_text":"fn main() {\n    old();\n}\n","new_text":"fn main() {\n    new();\n}\n"}),
    };
    let lines = tool_preview(&call, /*width*/ 48, std::path::Path::new("/project"));
    let text = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("old()"));
    assert!(text.contains("new()"));
    assert!(text.contains("src/main.rs"));
    insta::assert_snapshot!(text);
}
