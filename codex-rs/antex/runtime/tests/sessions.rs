use std::io::Write;

use antex_core::Content;
use antex_core::Message;
use antex_core::RawToolCall;
use antex_core::ToolOutcome;
use antex_core::ToolOutput;
use antex_core::ToolScope;
use antex_core::UserInput;
use antex_runtime::SessionStore;
use pretty_assertions::assert_eq;
use serde_json::json;

fn session_file(home: &std::path::Path, id: uuid::Uuid) -> std::path::PathBuf {
    std::fs::read_dir(home.join("sessions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join(format!("{id}.jsonl"))
}

#[test]
fn session_round_trip_preserves_messages_images_and_provider_state() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut session = store.create().unwrap();
    let id = session.id();
    let messages = vec![
        Message::User(UserInput {
            content: vec![
                Content::Text("inspect".into()),
                Content::Image {
                    media_type: "image/png".into(),
                    data: vec![1, 2, 3].into(),
                },
            ],
            tool_scope: ToolScope::Default,
        }),
        Message::Assistant {
            content: vec![Content::Continuation {
                provider: "fake".into(),
                data: vec![4, 5, 6].into(),
            }],
            tool_calls: vec![RawToolCall {
                id: "read-1".into(),
                name: "read".into(),
                arguments: "{}".into(),
            }],
        },
        Message::Tool(ToolOutput::new(
            "read-1".into(),
            ToolOutcome::Success,
            "contents".into(),
        )),
    ];
    for message in &messages {
        session.append(message).unwrap();
    }
    session
        .append_extension("status", json!({"text":"not model context"}))
        .unwrap();
    session.finish_turn().unwrap();
    drop(session);
    let replay: Vec<_> = store
        .open(id)
        .unwrap()
        .active_path()
        .unwrap()
        .into_iter()
        .map(|entry| entry.message)
        .collect();
    assert_eq!(replay, messages);
    assert_eq!(store.list().unwrap(), vec![id]);
    let records: Vec<serde_json::Value> = std::fs::read_to_string(session_file(home.path(), id))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        records
            .iter()
            .map(|record| record["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "session",
            "user",
            "assistant",
            "tool_call",
            "tool_result",
            "extension"
        ]
    );
}

#[test]
fn branching_appends_parent_links_without_copying_or_rewriting_history() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut session = store.create().unwrap();
    let first = session.append(&Message::User("first".into())).unwrap();
    session.append(&Message::User("main path".into())).unwrap();
    let path = session_file(home.path(), session.id());
    let original = std::fs::read(&path).unwrap();
    session.branch(first).unwrap();
    session
        .append(&Message::User("branch path".into()))
        .unwrap();
    assert!(std::fs::read(&path).unwrap().starts_with(&original));
    assert_eq!(
        session
            .active_path()
            .unwrap()
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>(),
        vec![
            Message::User("first".into()),
            Message::User("branch path".into())
        ]
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 5);
    assert!(session.branch(uuid::Uuid::nil()).is_err());
}

#[test]
fn torn_tail_recovery_preserves_every_committed_record() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut session = store.create().unwrap();
    session.append(&Message::User("committed".into())).unwrap();
    session.finish_turn().unwrap();
    let id = session.id();
    let path = session_file(home.path(), id);
    drop(session);
    let original = std::fs::read(&path).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"schema_version\":1")
        .unwrap();
    let mut reopened = store.open(id).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(
        reopened
            .active_path()
            .unwrap()
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>(),
        vec![Message::User("committed".into())]
    );
    reopened
        .append(&Message::User("after recovery".into()))
        .unwrap();
    drop(reopened);
    assert_eq!(store.open(id).unwrap().active_path().unwrap().len(), 2);
}

#[test]
fn simultaneous_writers_are_rejected_and_invalid_schema_is_not_repaired() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = SessionStore::new(home.path(), workspace.path()).unwrap();
    let session = store.create().unwrap();
    let id = session.id();
    assert!(store.open(id).is_err());
    let path = session_file(home.path(), id);
    drop(session);
    let original = std::fs::read(&path).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{}\n")
        .unwrap();
    let corrupt = std::fs::read(&path).unwrap();
    assert!(store.open(id).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), corrupt);
    assert!(corrupt.starts_with(&original));
}

#[test]
fn interrupted_tools_are_closed_without_reexecuting_or_rewriting_them() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let store = SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut session = store.create().unwrap();
    session.append(&Message::User("task".into())).unwrap();
    session
        .append(&Message::Assistant {
            content: Vec::new(),
            tool_calls: vec![RawToolCall {
                id: "pending".into(),
                name: "write".into(),
                arguments: "{}".into(),
            }],
        })
        .unwrap();
    let path = session_file(home.path(), session.id());
    let original = std::fs::read(&path).unwrap();
    assert_eq!(session.recover_pending_tools().unwrap(), 1);
    assert_eq!(session.recover_pending_tools().unwrap(), 0);
    assert!(std::fs::read(&path).unwrap().starts_with(&original));
    assert_eq!(session.active_path().unwrap().last().unwrap().message,Message::Tool(ToolOutput::new("pending".into(),ToolOutcome::Cancelled,"tool outcome was not recorded before interruption; inspect the workspace before retrying".into())));
}
