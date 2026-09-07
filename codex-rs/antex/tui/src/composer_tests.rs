use super::*;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

#[test]
fn stash_preserves_images_and_rejects_overwriting_an_active_draft() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    composer.paste("Describe ").unwrap();
    composer
        .attach("image/png".into(), Arc::from([1, 2, 3]))
        .unwrap();
    let original = composer.draft();
    composer.stash().unwrap();
    composer.paste("another draft").unwrap();
    assert!(composer.restore_stash().is_err());
    assert_eq!(composer.draft().input(), UserInput::from("another draft"));
    composer.accept_submission();
    composer.restore_stash().unwrap();
    assert_eq!(composer.draft(), original);
    assert_eq!(
        composer.draft().input(),
        UserInput {
            content: vec![
                Content::Text("Describe ".into()),
                Content::Image {
                    media_type: "image/png".into(),
                    data: Arc::from([1, 2, 3])
                }
            ],
            tool_scope: ToolScope::Default,
        }
    );
}

#[test]
fn deleting_an_image_marker_removes_only_that_attachment() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    composer.attach("image/png".into(), Arc::from([1])).unwrap();
    composer.attach("image/png".into(), Arc::from([2])).unwrap();
    composer
        .key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        composer.draft().input(),
        UserInput {
            content: vec![Content::Image {
                media_type: "image/png".into(),
                data: Arc::from([1])
            }],
            tool_scope: ToolScope::Default,
        }
    );
}

#[test]
fn sending_preserves_the_draft_until_acknowledged() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    composer.paste("keep me").unwrap();
    let before = composer.draft();
    assert_eq!(
        composer
            .key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap(),
        Some(SubmitMode::Send)
    );
    assert_eq!(composer.draft(), before);
    composer.accept_submission();
    assert!(composer.draft().input().content.is_empty());
}

#[test]
fn long_unicode_drafts_split_without_losing_text_or_exceeding_block_limits() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    let text = "👩‍💻 привет ".repeat(900);
    composer.paste(&text).unwrap();
    let input = composer.draft().input();
    let restored = input
        .content
        .iter()
        .map(|block| match block {
            Content::Text(text) => {
                assert!(text.len() <= MAX_TEXT_BYTES);
                text.as_str()
            }
            _ => panic!("unexpected block"),
        })
        .collect::<String>();
    assert_eq!(restored, text);
    let before = composer.draft();
    assert!(composer.paste(&"x".repeat(MAX_DRAFT_BYTES)).is_err());
    assert_eq!(composer.draft(), before);
}

#[test]
fn narrow_composer_keeps_unicode_cursor_inside_its_viewport() {
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    composer.paste("привет 👩‍💻\nnext").unwrap();
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 16, /*height*/ 3,
    );
    let mut buffer = Buffer::empty(area);
    let cursor = composer.render(area, &mut buffer).unwrap();
    assert!(area.contains(cursor.into()));
    let visible = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(visible, @"
    › привет 👩‍💻      
      next          
                    
    ");
}
