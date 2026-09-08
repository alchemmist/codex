use super::*;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

#[test]
fn transcript_pager_renders_markdown_and_requests_an_older_page() {
    let page = TextPage {
        title: "Transcript".into(),
        body: "## You\n\nInspect the file.\n\n## Antex\n\n**Done.**\n".into(),
        older_command: Some("/transcript older-id".into()),
    };
    let mut pager =
        Pager::new(page, "/project".into(), Arc::new(RuntimeKeymap::defaults())).unwrap();
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 48, /*height*/ 8,
    );
    let mut buffer = Buffer::empty(area);
    pager.render(area, &mut buffer);
    let text = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(text);
    let OverlayAction::Command(command) =
        pager.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
    else {
        panic!("expected older page");
    };
    assert_eq!(command, "/transcript older-id");
    assert!(matches!(
        pager.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        OverlayAction::Close
    ));
}
