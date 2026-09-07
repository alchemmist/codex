use std::collections::HashMap;
use std::sync::Arc;

use antex_core::ToolCall;
use antex_core::ToolContext;
use antex_core::ToolDefinition;
use antex_core::ToolHost;
use antex_core::ToolOutcome;
use antex_core::ToolOutput;
use antex_core::ToolScope;
use antex_extension_protocol::Manifest;
use futures::future::BoxFuture;

use crate::ExtensionRequest;
use crate::ExtensionResponse;
use crate::ManagedExtension;

pub struct HostedExtension {
    pub process: Arc<ManagedExtension>,
    pub manifest: Manifest,
}

struct ExtensionTool {
    process: Arc<ManagedExtension>,
    remote_name: String,
}

pub struct ExtensionToolHost {
    fallback: Arc<dyn ToolHost>,
    definitions: Vec<ToolDefinition>,
    tools: HashMap<String, ExtensionTool>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolHostError {
    #[error("extension tool conflicts with an enabled tool: {0}")]
    Duplicate(String),
    #[error("extension tool name exceeds its budget")]
    NameLimit,
}

impl ExtensionToolHost {
    pub fn new(
        fallback: Arc<dyn ToolHost>,
        extensions: Vec<HostedExtension>,
    ) -> Result<Self, ToolHostError> {
        let mut definitions = fallback.definitions(&ToolScope::Default);
        let mut tools = HashMap::new();
        for extension in extensions {
            for tool in extension.manifest.tools {
                let name = format!("{}__{}", extension.manifest.name, tool.name);
                if name.len() > 64 {
                    return Err(ToolHostError::NameLimit);
                }
                if definitions.iter().any(|definition| definition.name == name)
                    || tools.contains_key(&name)
                {
                    return Err(ToolHostError::Duplicate(name));
                }
                definitions.push(ToolDefinition {
                    name: name.clone(),
                    description: tool.description,
                    parameters: tool.parameters,
                });
                tools.insert(
                    name,
                    ExtensionTool {
                        process: Arc::clone(&extension.process),
                        remote_name: tool.name,
                    },
                );
            }
        }
        Ok(Self {
            fallback,
            definitions,
            tools,
        })
    }
}

impl ToolHost for ExtensionToolHost {
    fn definitions(&self, scope: &ToolScope) -> Vec<ToolDefinition> {
        if *scope == ToolScope::Default {
            self.definitions.clone()
        } else {
            self.fallback.definitions(scope)
        }
    }

    fn execute(&self, call: ToolCall, context: ToolContext) -> BoxFuture<'_, ToolOutput> {
        let Some(tool) = self.tools.get(&call.name) else {
            return self.fallback.execute(call, context);
        };
        let id = call.id;
        let request = antex_extension_protocol::ToolCall {
            name: tool.remote_name.clone(),
            arguments: call.arguments,
        };
        Box::pin(async move {
            match tool
                .process
                .request(
                    ExtensionRequest::Tool(request),
                    context.cancellation.clone(),
                )
                .await
            {
                Ok(ExtensionResponse::Output(output)) if output.actions.is_empty() => {
                    ToolOutput::new(id, ToolOutcome::Success, output.text)
                }
                Ok(ExtensionResponse::Output(_)) => ToolOutput::new(
                    id,
                    ToolOutcome::Failure,
                    "extension tools cannot publish host actions".into(),
                ),
                Ok(ExtensionResponse::Notified) => ToolOutput::new(
                    id,
                    ToolOutcome::Failure,
                    "extension tool returned no output".into(),
                ),
                Err(error) => ToolOutput::new(id, ToolOutcome::Failure, error.to_string()),
            }
        })
    }
}
