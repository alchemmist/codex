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

#[test]
fn interrupted_pending_inputs_survive_resume_without_replaying_consumed_inputs() {
    use antex_core::AgentCommand;
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = crate::SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut conversation = Conversation::new(store.create().unwrap()).unwrap();
    let id = conversation.id();
    let pending = vec![
        AgentCommand::Steer("first".into()),
        AgentCommand::FollowUp("second".into()),
    ];
    conversation
        .record(&AgentEvent::Finished {
            reason: FinishReason::Interrupted,
            pending: pending.clone(),
        })
        .unwrap();
    assert_eq!(conversation.pending_commands().unwrap(), pending);
    assert!(conversation.messages().is_empty());
    drop(conversation);
    let mut resumed = Conversation::new(store.open(id).unwrap()).unwrap();
    assert_eq!(resumed.pending_commands().unwrap(), pending);
    resumed
        .record(&AgentEvent::MessageCommitted(Message::User("first".into())))
        .unwrap();
    drop(resumed);
    let mut recovered = Conversation::new(store.open(id).unwrap()).unwrap();
    assert_eq!(recovered.pending_commands().unwrap(), pending[1..]);
    recovered
        .record(&AgentEvent::Finished {
            reason: FinishReason::Completed,
            pending: Vec::new(),
        })
        .unwrap();
    assert_eq!(recovered.pending_commands().unwrap(), pending[1..]);
}

#[test]
fn ui_state_survives_resume_and_never_enters_model_history() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = crate::SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut conversation = Conversation::new(store.create().unwrap()).unwrap();
    let id = conversation.id();
    let value = serde_json::json!({"text":"private unsent draft","elements":[]});
    conversation.save_ui_state("promptStash", &value).unwrap();
    assert!(conversation.messages().is_empty());
    drop(conversation);
    let mut resumed = Conversation::new(store.open(id).unwrap()).unwrap();
    assert_eq!(resumed.load_ui_state("promptStash").unwrap(), Some(value));
    assert!(resumed.messages().is_empty());
    resumed
        .save_ui_state("promptStash", &serde_json::Value::Null)
        .unwrap();
    assert_eq!(resumed.load_ui_state("promptStash").unwrap(), None);
}
