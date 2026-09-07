use super::*;
use pretty_assertions::assert_eq;

#[test]
fn paths_accept_terminal_quoting_without_treating_prose_as_an_attachment() {
    for source in [
        "'/tmp/my image.png'",
        "/tmp/my\\ image.png",
        "file:///tmp/my%20image.png",
    ] {
        assert_eq!(
            pasted_image_path(source),
            Some(PathBuf::from("/tmp/my image.png"))
        );
    }
    assert_eq!(pasted_image_path("explain the image.png"), None);
    assert_eq!(pasted_image_path("hello"), None);
    assert_eq!(parse_image_path("'unterminated.png"), None);
}
