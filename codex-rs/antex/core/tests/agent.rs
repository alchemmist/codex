use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use antex_core::*;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;
use serde_json::json;

#[path = "agent/interaction.rs"]
mod interactions;

#[derive(Clone)]
struct Provider {
    responses: Arc<Mutex<VecDeque<Vec<ModelEvent>>>>,
    requests: Arc<Mutex<Vec<ModelRequest>>>,
}

impl ModelProvider for Provider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(Vec::new())
    }

    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let response = self.responses.lock().unwrap().pop_front().unwrap();
        Ok(Box::pin(futures::stream::iter(
            response.into_iter().map(Ok),
        )))
    }
}

#[derive(Default)]
struct Tools {
    calls: Mutex<Vec<ToolCall>>,
    scopes: Mutex<Vec<ToolScope>>,
}

impl ToolHost for Tools {
    fn definitions(&self, scope: &ToolScope) -> Vec<ToolDefinition> {
        self.scopes.lock().unwrap().push(scope.clone());
        vec![ToolDefinition {
            name: "echo".into(),
            description: "Return text".into(),
            parameters: json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false}),
        }]
    }

    fn execute(&self, call: ToolCall, _context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(call.clone());
            ToolOutput::new(
                call.id,
                ToolOutcome::Success,
                call.arguments["text"].as_str().unwrap().into(),
            )
        })
    }
}

fn input() -> TurnInput {
    TurnInput {
        model: "fake".into(),
        reasoning: None,
        history: Vec::new(),
        input: "hello".into(),
    }
}

fn provider(responses: Vec<Vec<ModelEvent>>) -> Provider {
    Provider {
        responses: Arc::new(Mutex::new(responses.into())),
        requests: Arc::new(Mutex::new(Vec::new())),
    }
}

fn call(name: &str, arguments: &str) -> ModelEvent {
    ModelEvent::ToolCall(RawToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments: arguments.into(),
    })
}

async fn drain(mut run: AgentRun) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(event) = run.events.recv().await {
        events.push(event);
    }
    events
}

#[tokio::test]
async fn commits_tool_exchange_before_requesting_final_answer() {
    let provider = provider(vec![
        vec![
            call("echo", r#"{"text":"world"}"#),
            ModelEvent::Finished(Usage::default()),
        ],
        vec![
            ModelEvent::Text("done".into()),
            ModelEvent::Finished(Usage::default()),
        ],
    ]);
    let tools = Arc::new(Tools::default());
    let mut agent = Agent::new(provider.clone(), tools.clone());
    let events = drain(agent.start(input())).await;
    let expected_call = ToolCall {
        id: "call-1".into(),
        name: "echo".into(),
        arguments: json!({"text":"world"}),
    };
    assert_eq!(*tools.calls.lock().unwrap(), vec![expected_call]);
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].messages,
        vec![
            Message::User("hello".into()),
            Message::Assistant {
                content: Vec::new(),
                tool_calls: vec![RawToolCall {
                    id: "call-1".into(),
                    name: "echo".into(),
                    arguments: r#"{"text":"world"}"#.into()
                }]
            },
            Message::Tool(ToolOutput::new(
                "call-1".into(),
                ToolOutcome::Success,
                "world".into()
            )),
        ]
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Completed,
            pending: Vec::new()
        })
    );
}

#[tokio::test]
async fn rejects_unknown_and_malformed_tool_calls_without_execution() {
    for (name, arguments) in [
        ("missing", "{}"),
        ("echo", "not json"),
        ("echo", r#"{"text":42}"#),
    ] {
        let provider = provider(vec![
            vec![
                call(name, arguments),
                ModelEvent::Finished(Usage::default()),
            ],
            vec![ModelEvent::Finished(Usage::default())],
        ]);
        let tools = Arc::new(Tools::default());
        let mut agent = Agent::new(provider, tools.clone());
        let events = drain(agent.start(input())).await;
        assert_eq!(*tools.calls.lock().unwrap(), Vec::new());
        assert!(events.iter().any(|event| matches!(event, AgentEvent::MessageCommitted(Message::Tool(output)) if output.outcome == ToolOutcome::Failure)));
    }
}

#[tokio::test]
async fn truncated_stream_preserves_text_but_never_executes_calls() {
    let provider = provider(vec![vec![
        ModelEvent::Text("partial".into()),
        call("echo", r#"{"text":"unsafe"}"#),
    ]]);
    let tools = Arc::new(Tools::default());
    let mut agent = Agent::new(provider, tools.clone());
    let events = drain(agent.start(input())).await;
    assert_eq!(*tools.calls.lock().unwrap(), Vec::new());
    assert!(
        events.contains(&AgentEvent::MessageCommitted(Message::Assistant {
            content: vec![Content::Text("partial".into())],
            tool_calls: Vec::new()
        }))
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Failed,
            pending: Vec::new()
        })
    );
}

#[tokio::test]
async fn interrupt_bypasses_a_full_queue_and_returns_pending_input() {
    let mut agent = Agent::new(provider(Vec::new()), Arc::new(Tools::default()));
    let run = agent.start(input());
    let pending: Vec<_> = (0..32)
        .map(|number| AgentCommand::FollowUp(format!("queued {number}").into()))
        .collect();
    for command in &pending {
        run.commands.try_send(command.clone()).unwrap();
    }
    let rejected = AgentCommand::Steer("overflow".into());
    assert_eq!(
        run.commands.try_send(rejected.clone()).unwrap_err().command,
        rejected
    );
    run.commands.send(AgentCommand::Interrupt).await.unwrap();
    let events = drain(run).await;
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Interrupted,
            pending
        })
    );
}

#[tokio::test]
async fn followup_appends_after_completion_and_closed_queue_returns_input() {
    let provider = provider(vec![
        vec![ModelEvent::Finished(Usage::default())],
        vec![ModelEvent::Finished(Usage::default())],
    ]);
    let mut agent = Agent::new(provider.clone(), Arc::new(Tools::default()));
    let run = agent.start(input());
    let commands = run.commands.clone();
    commands
        .try_send(AgentCommand::FollowUp("second".into()))
        .unwrap();
    let events = drain(run).await;
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == AgentEvent::TurnCompleted)
            .count(),
        2
    );
    assert_eq!(
        provider.requests.lock().unwrap()[1].messages.last(),
        Some(&Message::User("second".into()))
    );
    let rejected = AgentCommand::FollowUp("too late".into());
    assert_eq!(
        commands.try_send(rejected.clone()).unwrap_err().command,
        rejected
    );
}

#[tokio::test]
async fn steering_precedes_followups_and_followups_reset_tool_scope() {
    let provider = provider(vec![
        vec![ModelEvent::Finished(Usage::default())],
        vec![ModelEvent::Finished(Usage::default())],
    ]);
    let tools = Arc::new(Tools::default());
    let mut agent = Agent::new(provider.clone(), tools.clone());
    let mut turn = input();
    turn.input.tool_scope = ToolScope::Named("delegation".into());
    let run = agent.start(turn);
    run.commands
        .try_send(AgentCommand::FollowUp("next".into()))
        .unwrap();
    run.commands
        .try_send(AgentCommand::Steer("correction".into()))
        .unwrap();
    drain(run).await;
    assert_eq!(
        *tools.scopes.lock().unwrap(),
        vec![ToolScope::Named("delegation".into()), ToolScope::Default]
    );
    assert_eq!(
        &provider.requests.lock().unwrap()[0].messages[1..],
        &[Message::User("correction".into())]
    );
}

struct WaitingTools {
    started: tokio::sync::Notify,
}

impl ToolHost for WaitingTools {
    fn definitions(&self, scope: &ToolScope) -> Vec<ToolDefinition> {
        Tools::default().definitions(scope)
    }

    fn execute(&self, _call: ToolCall, context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            self.started.notify_one();
            context.cancellation.cancelled().await;
            std::future::pending().await
        })
    }
}

#[tokio::test]
async fn interruption_finishes_every_committed_tool_call() {
    let second = ModelEvent::ToolCall(RawToolCall {
        id: "call-2".into(),
        name: "echo".into(),
        arguments: r#"{"text":"second"}"#.into(),
    });
    let provider = provider(vec![vec![
        call("echo", r#"{"text":"first"}"#),
        second,
        ModelEvent::Finished(Usage::default()),
    ]]);
    let tools = Arc::new(WaitingTools {
        started: tokio::sync::Notify::new(),
    });
    let mut agent = Agent::new(provider, tools.clone());
    let run = agent.start(input());
    tools.started.notified().await;
    run.commands.send(AgentCommand::Interrupt).await.unwrap();
    let events = drain(run).await;
    let outputs: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageCommitted(Message::Tool(output)) => Some(output.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        outputs,
        vec![
            ToolOutput::new(
                "call-1".into(),
                ToolOutcome::Cancelled,
                "tool call cancelled".into()
            ),
            ToolOutput::new(
                "call-2".into(),
                ToolOutcome::Cancelled,
                "tool call cancelled".into()
            ),
        ]
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Interrupted,
            pending: Vec::new()
        })
    );
}

#[tokio::test]
async fn a_second_start_does_not_overlap_or_destroy_an_active_run() {
    let provider = provider(vec![vec![ModelEvent::Finished(Usage::default())]]);
    let mut agent = Agent::new(provider, Arc::new(Tools::default()));
    let first = agent.start(input());
    let second = drain(agent.start(input())).await;
    assert_eq!(
        second.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Failed,
            pending: vec![AgentCommand::FollowUp("hello".into())]
        })
    );
    assert_eq!(
        drain(first).await.last(),
        Some(&AgentEvent::Finished {
            reason: FinishReason::Completed,
            pending: Vec::new()
        })
    );
}

#[tokio::test]
async fn quota_events_are_presented_without_becoming_model_history() {
    let quota = Quota {
        primary: Some(QuotaWindow {
            used_basis_points: 5000,
            window_seconds: Some(3600),
            resets_at: Some(10000),
        }),
        secondary: None,
        credits: None,
    };
    let provider = provider(vec![
        vec![
            ModelEvent::Quota(quota.clone()),
            ModelEvent::Finished(Usage::default()),
        ],
        vec![ModelEvent::Finished(Usage::default())],
    ]);
    let mut agent = Agent::new(provider.clone(), Arc::new(Tools::default()));
    let run = agent.start(input());
    run.commands
        .try_send(AgentCommand::FollowUp("next".into()))
        .unwrap();
    let events = drain(run).await;
    assert!(events.contains(&AgentEvent::Quota(quota)));
    assert_eq!(
        provider.requests.lock().unwrap()[1].messages,
        vec![
            Message::User("hello".into()),
            Message::Assistant {
                content: Vec::new(),
                tool_calls: Vec::new()
            },
            Message::User("next".into())
        ]
    );
}
