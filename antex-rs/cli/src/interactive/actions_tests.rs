use std::sync::Arc;

use antex_core::ContextHook;
use antex_core::ModelInfo;
use antex_core::ModelProvider;
use antex_core::ModelRequest;
use antex_core::ModelStream;
use antex_core::PreparedContext;
use antex_core::ProviderError;
use antex_core::ToolCall;
use antex_core::ToolContext;
use antex_core::ToolDefinition;
use antex_core::ToolHost;
use antex_core::ToolOutput;
use antex_core::ToolScope;
use antex_core::Usage;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;

use super::*;

struct BarrierProvider(Arc<tokio::sync::Barrier>);

impl ModelProvider for BarrierProvider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        unreachable!()
    }

    async fn stream(&self, _request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.0.wait().await;
        Ok(Box::pin(futures::stream::iter([
            Ok(antex_core::ModelEvent::Text("done".into())),
            Ok(antex_core::ModelEvent::Finished(Usage::default())),
        ])))
    }
}

struct EmptyTools;

impl ToolHost for EmptyTools {
    fn definitions(&self, _scope: &ToolScope) -> Vec<ToolDefinition> {
        Vec::new()
    }

    fn execute(&self, _call: ToolCall, _context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        unreachable!()
    }
}

struct EmptyContext;

impl ContextHook for EmptyContext {
    fn prepare<'a>(
        &'a self,
        history: &'a [Message],
    ) -> BoxFuture<'a, Result<PreparedContext, ProviderError>> {
        Box::pin(async move { Ok(history.to_vec().into()) })
    }
}

#[tokio::test]
async fn agent_batch_starts_independent_agents_concurrently_and_preserves_order() {
    let actions = vec![
        Action::Agent {
            id: "first".into(),
            prompt: "one".into(),
            model: Some("test".into()),
        },
        Action::Agent {
            id: "second".into(),
            prompt: "two".into(),
            model: Some("test".into()),
        },
    ];
    let results = tokio::time::timeout(
        Duration::from_secs(/*secs*/ 1),
        run_agents(
            Arc::new(BarrierProvider(Arc::new(tokio::sync::Barrier::new(2)))),
            Arc::new(EmptyTools),
            Arc::new(EmptyContext),
            None,
            None,
            actions,
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        results
            .into_iter()
            .map(|result| (result.id, result.succeeded, result.data["text"].clone()))
            .collect::<Vec<_>>(),
        vec![
            ("first".into(), true, serde_json::json!("done")),
            ("second".into(), true, serde_json::json!("done")),
        ]
    );
}
