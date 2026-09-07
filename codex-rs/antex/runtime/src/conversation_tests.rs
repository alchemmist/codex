use super::*;
use antex_core::ContextCheckpoint;
use antex_core::ContextFragment;
use antex_core::ContextKind;
use antex_core::FinishReason;
use pretty_assertions::assert_eq;

#[test]
fn checkpoints_keep_record_ids_aligned_across_turns_and_resume() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = crate::SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut conversation = Conversation::new(store.create().unwrap()).unwrap();
    let id = conversation.id();
    for text in ["first", "second", "third"] {
        conversation
            .record(&AgentEvent::MessageCommitted(Message::User(text.into())))
            .unwrap();
    }
    let original = conversation.entries().to_vec();
    let summary =
        ContextFragment::new(ContextKind::Summary, "first two turns summarized".into()).unwrap();
    conversation
        .record(&AgentEvent::ContextCheckpoint(ContextCheckpoint {
            summary: summary.clone(),
            retained: vec![1],
            tail_start: 2,
            usage: Default::default(),
        }))
        .unwrap();
    assert_eq!(&conversation.entries()[1..], &original[1..]);
    let projected = conversation.entries().to_vec();
    conversation
        .record(&AgentEvent::ContextCheckpoint(ContextCheckpoint {
            summary,
            retained: vec![1],
            tail_start: 2,
            usage: Default::default(),
        }))
        .unwrap();
    assert_eq!(conversation.entries(), projected);
    conversation
        .record(&AgentEvent::MessageCommitted(Message::User(
            "fourth".into(),
        )))
        .unwrap();
    conversation
        .record(&AgentEvent::Finished {
            reason: FinishReason::Completed,
            pending: Vec::new(),
        })
        .unwrap();
    let expected = conversation.entries().to_vec();
    drop(conversation);
    let mut resumed = Conversation::new(store.open(id).unwrap()).unwrap();
    assert_eq!(resumed.entries(), expected);
    resumed.branch(original[1].record_id).unwrap();
    assert_eq!(resumed.entries(), &original[..2]);
}

#[test]
fn invalid_checkpoint_cannot_advance_the_session() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = crate::SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut conversation = Conversation::new(store.create().unwrap()).unwrap();
    conversation
        .record(&AgentEvent::MessageCommitted(Message::User("keep".into())))
        .unwrap();
    let before = conversation.entries().to_vec();
    assert!(
        conversation
            .record(&AgentEvent::ContextCheckpoint(ContextCheckpoint {
                summary: ContextFragment::new(ContextKind::Summary, "invalid".into()).unwrap(),
                retained: vec![99],
                tail_start: 0,
                usage: Default::default(),
            }))
            .is_err()
    );
    assert_eq!(conversation.entries(), before);
}
