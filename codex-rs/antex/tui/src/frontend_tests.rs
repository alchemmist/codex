use super::*;
use antex_core::*;
use crossterm::event::KeyEvent;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;
use tokio::sync::Notify;
use tokio::sync::mpsc;

struct Provider;

impl ModelProvider for Provider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }

    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        assert_eq!(request.messages, vec![Message::User("hello".into())]);
        Ok(Box::pin(futures::stream::iter([
            Ok(ModelEvent::Text("Hello from the kernel.".into())),
            Ok(ModelEvent::Finished(Usage::default())),
        ])))
    }
}

struct Tools;

impl ToolHost for Tools {
    fn definitions(&self, _scope: &ToolScope) -> Vec<ToolDefinition> {
        Vec::new()
    }
    fn execute(&self, _call: ToolCall, _context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async { panic!("no tools requested") })
    }
}

struct TestSession<P: ModelProvider + 'static> {
    agent: Agent<P>,
    messages: Vec<Message>,
    finished: Arc<Notify>,
    requested: Arc<Notify>,
}

impl<P: ModelProvider + 'static> Session for TestSession<P> {
    fn load_ui_state(&mut self, _name: &str) -> Result<Option<serde_json::Value>, String> {
        Ok(None)
    }
    fn save_ui_state(&mut self, _name: &str, _value: &serde_json::Value) -> Result<(), String> {
        Ok(())
    }
    fn pending_commands(&mut self) -> Result<Vec<AgentCommand>, String> {
        Ok(Vec::new())
    }
    fn view(&self) -> crate::SessionView {
        crate::SessionView {
            model: "fake".into(),
            directory: "/project".into(),
            permissions: "workspace".into(),
            session_id: "test".into(),
        }
    }
    fn history(&self) -> Vec<Message> {
        self.messages.clone()
    }
    async fn start(&mut self, input: UserInput) -> Result<AgentRun, String> {
        Ok(self.agent.start(TurnInput {
            model: "fake".into(),
            reasoning: None,
            history: self.history(),
            input,
        }))
    }
    fn record(&mut self, event: &AgentEvent) -> Result<(), String> {
        match event {
            AgentEvent::MessageCommitted(message) => self.messages.push(message.clone()),
            AgentEvent::Finished { .. } => self.finished.notify_one(),
            AgentEvent::Interaction { .. } => self.requested.notify_one(),
            _ => {}
        }
        Ok(())
    }
    async fn command(&mut self, _command: &str) -> Result<CommandEffect, String> {
        Err("unknown command".into())
    }
}

#[tokio::test]
async fn real_kernel_events_reach_inline_terminal_and_persistence() {
    let finished = Arc::new(Notify::new());
    let mut session = TestSession {
        agent: Agent::new(Provider, Arc::new(Tools)),
        messages: Vec::new(),
        finished: finished.clone(),
        requested: Arc::new(Notify::new()),
    };
    let backend = crate::test_backend::VT100Backend::with_scrollback(
        /*width*/ 48, /*height*/ 16, /*scrollback_len*/ 100,
    );
    let mut tui = Tui::new(backend).unwrap();
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
        session.messages,
        vec![
            Message::User("hello".into()),
            Message::Assistant {
                content: vec![Content::Text("Hello from the kernel.".into())],
                tool_calls: Vec::new()
            }
        ]
    );
    assert!(!tui.terminal.backend().vt100().screen().alternate_screen());
    let visible = tui.terminal.backend().vt100().screen().contents();
    assert!(visible.contains("Hello from the kernel."), "{visible}");
    insta::assert_snapshot!(visible);
}

#[path = "frontend_interaction_tests.rs"]
mod interactions;
