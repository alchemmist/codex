use super::*;
use crossterm::event::KeyEvent;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn cancellation_preserves_typeahead_without_waiting_for_the_request() {
    let events = vec![
        Ok(Event::Paste("typed ahead".into())),
        Ok(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))),
    ];
    let mut input = futures::stream::iter(events);
    let mut buffered = VecDeque::new();
    let mut state = InputState::Open;
    let result = wait(
        std::future::pending::<Result<(), String>>(),
        &mut input,
        &mut buffered,
        &mut state,
    )
    .await;
    assert_eq!(result, Err("Cancelled.".into()));
    assert!(state == InputState::Open);
    assert_eq!(
        buffered.pop_front().unwrap().unwrap(),
        Event::Paste("typed ahead".into())
    );
    assert!(buffered.is_empty());
}
