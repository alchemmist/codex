use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn image_commands_normalize_selected_files_without_contacting_the_provider() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config::default(),
        /*bubblewrap*/ None,
        provider,
    )
    .unwrap();
    let pixels = vec![255, 0, 0, 255, 0, 255, 0, 128];
    let expected = antex_runtime::ImageAttachment::from_rgba(
        /*width*/ 2,
        /*height*/ 1,
        pixels.clone(),
    )
    .unwrap()
    .content;
    assert_eq!(
        session
            .prepare_image(antex_tui::ImageSource::Rgba {
                width: 2,
                height: 1,
                bytes: pixels
            })
            .await
            .unwrap(),
        expected
    );
    let antex_core::Content::Image { data, .. } = &expected else {
        panic!("expected normalized image");
    };
    std::fs::write(workspace.path().join("my image.png"), data).unwrap();
    let CommandEffect::Image(actual) = session.command("/image 'my image.png'").await.unwrap()
    else {
        panic!("expected attachment");
    };
    assert_eq!(actual, expected);
    assert!(session.history().is_empty());
}
