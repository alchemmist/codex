use super::*;
use pretty_assertions::assert_eq;

struct ApprovalProvider;

impl ModelProvider for ApprovalProvider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }
    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let event = if let Some(Message::Tool(output)) = request.messages.last() {
            ModelEvent::Text(output.text().to_owned())
        } else {
            ModelEvent::ToolCall(RawToolCall {
                id: "approval".into(),
                name: "approval".into(),
                arguments: "{}".into(),
            })
        };
        Ok(Box::pin(futures::stream::iter([
            Ok(event),
            Ok(ModelEvent::Finished(Usage::default())),
        ])))
    }
}

struct ApprovalTool;

impl ToolHost for ApprovalTool {
    fn definitions(&self, _scope: &ToolScope) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "approval".into(),
            description: "Ask before executing".into(),
            parameters: serde_json::json!({"type":"object","additionalProperties":false}),
        }]
    }
    fn execute(&self, call: ToolCall, context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let answer = context
                .interact(InteractionPrompt::Approval {
                    action: "Run the requested privileged command?".into(),
                })
                .await
                .unwrap();
            ToolOutput::new(call.id, ToolOutcome::Success, format!("{answer:?}"))
        })
    }
}

struct WaitingProvider(Arc<Notify>);

#[tokio::test]
async fn escape_interrupts_a_running_turn_without_closing_the_terminal() {
    let started = Arc::new(Notify::new());
    let finished = Arc::new(Notify::new());
    let mut session = TestSession {
        agent: Agent::new(WaitingProvider(started.clone()), Arc::new(Tools)),
        messages: Vec::new(),
        finished: finished.clone(),
        requested: Arc::new(Notify::new()),
    };
    let mut tui = Tui::new(crate::test_backend::VT100Backend::new(80, 20)).unwrap();
    let (sender, receiver) = mpsc::channel(4);
    let input = Box::pin(futures::stream::unfold(
        receiver,
        |mut receiver| async move { receiver.recv().await.map(|event| (event, receiver)) },
    ));
    let driver = async {
        sender.send(Ok(Event::Paste("hello".into()))).await.unwrap();
        sender
            .send(Ok(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))))
            .await
            .unwrap();
        started.notified().await;
        sender
            .send(Ok(Event::Key(KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            ))))
            .await
            .unwrap();
        finished.notified().await;
        sender.send(Ok(Event::Paste("/quit".into()))).await.unwrap();
        sender
            .send(Ok(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))))
            .await
            .unwrap();
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        tokio::join!(
            run_terminal(&mut tui, &mut session, input, crate::Settings::default()),
            driver
        )
    })
    .await
    .unwrap();
    result.unwrap();
    assert_eq!(session.messages, vec![Message::User("hello".into())]);
}

impl ModelProvider for WaitingProvider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }
    async fn stream(&self, _request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.0.notify_one();
        Ok(Box::pin(futures::stream::pending()))
    }
}

#[tokio::test]
async fn approval_prompt_snapshot_uses_the_terminal_safe_accent() {
    let mut agent = Agent::new(ApprovalProvider, Arc::new(ApprovalTool));
    let mut run = agent.start(antex_core::TurnInput {
        model: "test".into(),
        reasoning: None,
        history: Vec::new(),
        input: "approve".into(),
    });
    let request = loop {
        match run.events.recv().await.unwrap() {
            AgentEvent::Interaction { request, .. } => break request,
            AgentEvent::ContextCheckpoint(_)
            | AgentEvent::MessageCommitted(_)
            | AgentEvent::TextDelta(_)
            | AgentEvent::ReasoningDelta(_)
            | AgentEvent::ToolStarted(_)
            | AgentEvent::ToolProgress { .. }
            | AgentEvent::Quota(_)
            | AgentEvent::Usage(_)
            | AgentEvent::TurnCompleted
            | AgentEvent::Error(_)
            | AgentEvent::Finished { .. } => {}
        }
    };
    let prompt = crate::prompt::Prompt::new(request.clone());
    insta::assert_debug_snapshot!(prompt.description());
    request.answer(InteractionAnswer::Deny).unwrap();
}

#[tokio::test]
async fn terminal_eof_interrupts_and_drains_the_active_run() {
    let started = Arc::new(Notify::new());
    let finished = Arc::new(Notify::new());
    let mut session = TestSession {
        agent: Agent::new(WaitingProvider(started.clone()), Arc::new(Tools)),
        messages: Vec::new(),
        finished: finished.clone(),
        requested: Arc::new(Notify::new()),
    };
    let mut tui = Tui::new(crate::test_backend::VT100Backend::new(
        /*width*/ 48, /*height*/ 16,
    ))
    .unwrap();
    let (sender, receiver) = mpsc::channel(4);
    let input = Box::pin(futures::stream::unfold(
        receiver,
        |mut receiver| async move { receiver.recv().await.map(|event| (event, receiver)) },
    ));
    let driver = async {
        sender.send(Ok(Event::Paste("hello".into()))).await.unwrap();
        sender
            .send(Ok(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))))
            .await
            .unwrap();
        started.notified().await;
        drop(sender);
        finished.notified().await;
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            run_terminal(&mut tui, &mut session, input, crate::Settings::default()),
            driver
        )
    })
    .await
    .unwrap();
    result.unwrap();
    assert_eq!(
        session.messages.first(),
        Some(&Message::User("hello".into()))
    );
}

#[tokio::test]
async fn approval_defaults_to_denial_and_requires_explicit_allow_selection() {
    for (keys, answer) in [
        (vec![KeyCode::Enter], "Deny"),
        (vec![KeyCode::Down, KeyCode::Enter], "AllowOnce"),
    ] {
        let requested = Arc::new(Notify::new());
        let finished = Arc::new(Notify::new());
        let mut session = TestSession {
            agent: Agent::new(ApprovalProvider, Arc::new(ApprovalTool)),
            messages: Vec::new(),
            finished: finished.clone(),
            requested: requested.clone(),
        };
        let mut tui = Tui::new(crate::test_backend::VT100Backend::with_scrollback(
            /*width*/ 60, /*height*/ 20, /*scrollback_len*/ 100,
        ))
        .unwrap();
        let (sender, receiver) = mpsc::channel(8);
        let input = Box::pin(futures::stream::unfold(
            receiver,
            |mut receiver| async move { receiver.recv().await.map(|event| (event, receiver)) },
        ));
        let driver = async {
            sender
                .send(Ok(Event::Paste("approve".into())))
                .await
                .unwrap();
            sender
                .send(Ok(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::NONE,
                ))))
                .await
                .unwrap();
            requested.notified().await;
            for code in keys {
                sender
                    .send(Ok(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))))
                    .await
                    .unwrap();
            }
            finished.notified().await;
            sender.send(Ok(Event::Paste("/quit".into()))).await.unwrap();
            sender
                .send(Ok(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::NONE,
                ))))
                .await
                .unwrap();
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(
                run_terminal(&mut tui, &mut session, input, crate::Settings::default()),
                driver
            )
        })
        .await
        .unwrap();
        result.unwrap();
        assert_eq!(
            session.messages.last(),
            Some(&Message::Assistant {
                content: vec![Content::Text(answer.into())],
                tool_calls: Vec::new()
            })
        );
        let visible = tui.terminal.backend().vt100().screen().contents();
        assert!(visible.contains("Approval required"), "{visible}");
        assert!(visible.contains(answer), "{visible}");
    }
}
