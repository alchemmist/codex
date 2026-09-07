use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ChatComposer;
use crate::render::renderable::Renderable;
use crate::terminal_hyperlinks::strip_osc8;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn composer_wrapped_url_fragments_keep_the_complete_destination() {
    let url = "https://github.com/openai/codex/pull/20252";
    let (sender, _receiver) = unbounded_channel::<AppEvent>();
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        AppEventSender::new(sender),
        /*enhanced_keys_supported*/ true,
        "Ask Codex to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_text_content(format!("Fix CI on {url}"), Vec::new(), Vec::new());
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 34, /*height*/ 8,
    );
    let mut buf = Buffer::empty(area);
    composer.render(area, &mut buf);
    let linked = buf
        .area
        .positions()
        .filter_map(|position| {
            let symbol = buf[position].symbol();
            let (destination, _) = symbol.strip_prefix("\x1b]8;;")?.split_once('\x07')?;
            Some((position.y, strip_osc8(symbol), destination.to_string()))
        })
        .collect::<Vec<_>>();
    assert!(!linked.is_empty());
    assert!(linked.iter().all(|(_, _, target)| target == url));
    assert_eq!(
        linked
            .iter()
            .map(|(_, text, _)| text.as_str())
            .collect::<String>(),
        url
    );
    assert!(linked.windows(2).any(|pair| pair[0].0 != pair[1].0));
}
