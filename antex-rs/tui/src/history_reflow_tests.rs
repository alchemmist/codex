use super::*;
use pretty_assertions::assert_eq;

#[test]
fn resize_rebuilds_user_rails_from_source_without_duplicate_history() {
    let backend = crate::test_backend::VT100Backend::with_scrollback(100, 30, 1000);
    let mut tui = crate::tui::Tui::new(backend).unwrap();
    tui.terminal.set_viewport_area(Rect::new(0, 0, 100, 5));
    let view = crate::SessionView {
        model: "fake".into(),
        directory: "/project".into(),
        permissions: "Workspace".into(),
        session_id: "test".into(),
    };
    let history = vec![
        Message::User("Привет! Ответь одним коротким предложением без инструментов.".into()),
        Message::Assistant {
            content: vec![antex_core::Content::Text("Привет! Чем могу помочь?".into())],
            tool_calls: Vec::new(),
        },
    ];
    replay(&mut tui, &history, None, &view, 5).unwrap();
    tui.terminal.backend_mut().set_size(32, 16);
    replay(&mut tui, &history, None, &view, 5).unwrap();
    let visible = tui.terminal.backend().vt100().screen().contents();
    assert_eq!(visible.matches("Привет! Ответь").count(), 1);
    assert!(
        visible.contains(" ┃ коротким предложением без"),
        "{visible}"
    );
    assert!(visible.contains(" ┃ инструментов."), "{visible}");
    insta::assert_snapshot!(visible);
    replay(&mut tui, &history, None, &view, 5).unwrap();
    assert_eq!(tui.terminal.backend().vt100().screen().contents(), visible);
}
