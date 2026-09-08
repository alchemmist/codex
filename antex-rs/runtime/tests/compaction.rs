use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use antex_core::*;
use antex_runtime::Compaction;
use antex_runtime::LocalRuntime;
use antex_runtime::PermissionProfile;
use antex_runtime::ProjectContext;
use antex_runtime::SessionStore;
use pretty_assertions::assert_eq;

enum Mode {
    Summary,
    Failure,
    Waiting,
}

struct Provider {
    mode: Mode,
    requests: Mutex<Vec<ModelRequest>>,
    started: tokio::sync::Notify,
    dropped: Arc<AtomicBool>,
}

impl ModelProvider for Provider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }
    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        match self.mode {
            Mode::Summary => Ok(Box::pin(futures::stream::iter(vec![
                Ok(ModelEvent::Text("condensed history".into())),
                Ok(ModelEvent::Finished(Usage {
                    input_tokens: 100,
                    output_tokens: 10,
                    cached_input_tokens: 0,
                })),
            ]))),
            Mode::Failure => Err(ProviderError {
                kind: ErrorKind::Transport,
                message: "offline".into(),
            }),
            Mode::Waiting => {
                let _guard = DropGuard(self.dropped.clone());
                self.started.notify_one();
                std::future::pending().await
            }
        }
    }
}

struct DropGuard(Arc<AtomicBool>);
impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn provider(mode: Mode) -> Arc<Provider> {
    Arc::new(Provider {
        mode,
        requests: Mutex::new(Vec::new()),
        started: tokio::sync::Notify::new(),
        dropped: Arc::new(AtomicBool::new(false)),
    })
}

fn history() -> Vec<Message> {
    let mut messages = vec![Message::User("preserve this exact user goal".into())];
    for index in 0..12 {
        let id = format!("call-{index}");
        messages.push(Message::Assistant {
            content: Vec::new(),
            tool_calls: vec![RawToolCall {
                id: id.clone(),
                name: "read".into(),
                arguments: "{}".into(),
            }],
        });
        messages.push(Message::Tool(ToolOutput::new(
            id,
            ToolOutcome::Success,
            "x".repeat(600),
        )));
    }
    messages
}

#[tokio::test]
async fn compaction_preserves_user_intent_and_complete_recent_tool_exchanges() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".git")).unwrap();
    let provider = provider(Mode::Summary);
    let compaction = Compaction::new(
        provider.clone(),
        ProjectContext::load(home.path(), workspace.path()).unwrap(),
        "fake".into(),
        2000,
    );
    let history = history();
    let prepared = compaction.prepare(&history).await.unwrap();
    let checkpoint = prepared.checkpoint.unwrap();
    assert_eq!(checkpoint.retained, vec![0]);
    assert!(matches!(
        history[checkpoint.tail_start],
        Message::Assistant { .. }
    ));
    let expected: Vec<_> = std::iter::once(Message::Context(checkpoint.summary.clone()))
        .chain(std::iter::once(history[0].clone()))
        .chain(history[checkpoint.tail_start..].iter().cloned())
        .collect();
    let actual:Vec<_>=prepared.messages.into_iter().filter(|message|!matches!(message,Message::Context(fragment) if fragment.kind()!=ContextKind::Summary)).collect();
    assert_eq!(actual, expected);
    let again = compaction.prepare(&history).await.unwrap();
    assert_eq!(again.checkpoint.unwrap().usage, Usage::default());
    assert_eq!(provider.requests.lock().unwrap().len(), 1);
    assert!(provider.requests.lock().unwrap()[0].tools.is_empty());

    let store = SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut session = store.create().unwrap();
    let ids = history
        .iter()
        .map(|message| session.append(message).unwrap())
        .collect::<Vec<_>>();
    assert!(
        session
            .checkpoint(
                checkpoint.summary.clone(),
                &[ids[0]],
                ids[checkpoint.tail_start]
            )
            .unwrap()
            .is_some()
    );
    assert_eq!(
        session
            .active_path()
            .unwrap()
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(
        session
            .checkpoint(
                checkpoint.summary.clone(),
                &[ids[0]],
                ids[checkpoint.tail_start]
            )
            .unwrap()
            .is_none()
    );
    session
        .append(&Message::User("next request".into()))
        .unwrap();
    assert!(
        session
            .checkpoint(checkpoint.summary, &[ids[0]], ids[checkpoint.tail_start])
            .unwrap()
            .is_none()
    );
    let id = session.id();
    drop(session);
    let reopened = store.open(id).unwrap().active_path().unwrap();
    assert_eq!(
        reopened.last().unwrap().message,
        Message::User("next request".into())
    );
    assert_eq!(
        &reopened
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>()[..expected.len()],
        expected.as_slice()
    );
}

#[tokio::test]
async fn failed_summary_does_not_create_a_cached_checkpoint() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".git")).unwrap();
    let provider = provider(Mode::Failure);
    let compaction = Compaction::new(
        provider.clone(),
        ProjectContext::load(home.path(), workspace.path()).unwrap(),
        "fake".into(),
        2000,
    );
    for _ in 0..2 {
        assert_eq!(
            compaction.prepare(&history()).await.err().unwrap().kind,
            ErrorKind::Transport
        );
    }
    assert_eq!(provider.requests.lock().unwrap().len(), 6);
}

#[tokio::test]
async fn interrupt_cancels_an_inflight_summary_request() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".git")).unwrap();
    let provider = provider(Mode::Waiting);
    let compaction = Compaction::new(
        provider.clone(),
        ProjectContext::load(home.path(), workspace.path()).unwrap(),
        "fake".into(),
        2000,
    );
    let runtime =
        Arc::new(LocalRuntime::new(workspace.path(), PermissionProfile::ReadOnly).unwrap());
    let mut agent = Agent::new(provider.clone(), runtime).with_context_hook(Arc::new(compaction));
    let mut run = agent.start(TurnInput {
        model: "fake".into(),
        reasoning: None,
        history: history(),
        input: "continue".into(),
    });
    provider.started.notified().await;
    run.commands.send(AgentCommand::Interrupt).await.unwrap();
    let mut completed = None;
    while let Some(event) = run.events.recv().await {
        assert!(!matches!(event, AgentEvent::ContextCheckpoint(_)));
        if let AgentEvent::Finished { reason, .. } = event {
            completed = Some(reason);
        }
    }
    assert_eq!(completed, Some(FinishReason::Interrupted));
    assert!(provider.dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn image_heavy_history_is_compacted_before_the_model_request_limit() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".git")).unwrap();
    let image = Content::Image {
        media_type: "image/png".into(),
        data: vec![0; 3 * 1024 * 1024].into(),
    };
    let previous = UserInput {
        content: vec![
            Content::Text("earlier".into()),
            image.clone(),
            image.clone(),
            image.clone(),
        ],
        tool_scope: ToolScope::Default,
    };
    let latest = UserInput {
        content: vec![
            Content::Text("latest".into()),
            image.clone(),
            image.clone(),
            image,
        ],
        tool_scope: ToolScope::Default,
    };
    let history = vec![
        Message::User(previous),
        Message::Assistant {
            content: vec![Content::Text("earlier findings".into())],
            tool_calls: Vec::new(),
        },
    ];
    let provider = provider(Mode::Summary);
    let compaction = Compaction::new(
        provider.clone(),
        ProjectContext::load(home.path(), workspace.path()).unwrap(),
        "fake".into(),
        64_000,
    );
    let runtime =
        Arc::new(LocalRuntime::new(workspace.path(), PermissionProfile::ReadOnly).unwrap());
    let mut agent = Agent::new(provider.clone(), runtime).with_context_hook(Arc::new(compaction));
    let store = SessionStore::new(home.path(), workspace.path()).unwrap();
    let mut session = store.create().unwrap();
    let mut ids = history
        .iter()
        .map(|message| session.append(message).unwrap())
        .collect::<Vec<_>>();
    let mut run = agent.start(TurnInput {
        model: "fake".into(),
        reasoning: None,
        history,
        input: latest.clone(),
    });
    let mut finished = None;
    while let Some(event) = run.events.recv().await {
        match event {
            AgentEvent::MessageCommitted(message) => ids.push(session.append(&message).unwrap()),
            AgentEvent::ContextCheckpoint(checkpoint) => {
                let retained = checkpoint
                    .retained
                    .iter()
                    .map(|index| ids[*index])
                    .collect::<Vec<_>>();
                let id = session
                    .checkpoint(checkpoint.summary, &retained, ids[checkpoint.tail_start])
                    .unwrap()
                    .unwrap();
                ids = std::iter::once(id)
                    .chain(retained)
                    .chain(ids[checkpoint.tail_start..].iter().copied())
                    .collect();
            }
            AgentEvent::Finished { reason, .. } => finished = Some(reason),
            _ => {}
        }
    }
    assert_eq!(finished, Some(FinishReason::Completed));
    let replay = session.active_path().unwrap();
    assert_eq!(replay.len(), 3);
    assert_eq!(replay[1].message, Message::User(latest));
    assert!(
        provider
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| context_size(&request.messages).unwrap() <= MAX_TRANSCRIPT_BYTES)
    );
}
