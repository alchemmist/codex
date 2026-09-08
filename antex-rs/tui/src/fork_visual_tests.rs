use antex_core::Message;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::sync::Arc;

#[test]
fn transcript_preserves_fork_message_gutters() {
    let render = |message: &Message| {
        crate::transcript::message_lines(message, 80, std::path::Path::new("/project"))
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        render(&Message::User("hello".into())),
        vec!["", " ┃ hello", ""]
    );
    assert_eq!(
        render(&Message::Assistant {
            content: vec![antex_core::Content::Text("Hello from the kernel.".into())],
            tool_calls: vec![]
        }),
        vec!["• Hello from the kernel.", ""]
    );
}

#[test]
fn composer_preserves_fork_insets_and_rail() {
    let mut composer =
        crate::composer::Composer::new(Arc::new(crate::keymap::RuntimeKeymap::defaults()));
    let area = Rect::new(0, 0, 40, 5);
    let mut buffer = Buffer::empty(area);
    let cursor = composer.render(area, &mut buffer);
    let lines = (0..5)
        .map(|y| {
            (0..40)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec!["", " ┃ Ask Antex to do anything", " ┃", " ┃", ""]
    );
    assert_eq!(cursor, Some((3, 1)));
}

#[test]
fn configured_fork_footer_keeps_directory_before_model() {
    let settings = crate::Settings::from_config(&serde_json::json!({"status_line":["current-dir","model"],"theme":"ansi","vim_mode_default":true,"vim_mode_start":"insert"}), "/test-home".into()).unwrap();
    assert_eq!(settings.warnings, Vec::<String>::new());
    let mut composer =
        crate::composer::Composer::new(Arc::new(crate::keymap::RuntimeKeymap::defaults()));
    composer.configure(&settings);
    assert_eq!(
        composer
            .footer(&crate::SessionView {
                model: "gpt-6-astra".into(),
                directory: "/project".into(),
                permissions: "Workspace".into(),
                session_id: "test".into()
            })
            .to_string(),
        "  /project · gpt-6-astra"
    );
    assert_eq!(
        crate::style::accent_style_for(Some((255, 255, 255))).bg,
        None
    );
    assert_eq!(crate::style::accent_style_for(Some((0, 0, 0))).bg, None);
}
