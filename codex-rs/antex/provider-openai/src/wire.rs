use antex_core::Content;
use antex_core::ErrorKind;
use antex_core::Message;
use antex_core::ModelEvent;
use antex_core::ModelRequest;
use antex_core::ProviderError;
use antex_core::RawToolCall;
use antex_core::Usage;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use serde_json::json;

const PROVIDER: &str = "openai";
pub(crate) const COMPATIBILITY_REVISION: &str = "0.153.4";

pub(crate) fn error(kind: ErrorKind, message: &str) -> ProviderError {
    ProviderError {
        kind,
        message: message.into(),
    }
}

pub(crate) fn encode(request: ModelRequest) -> Result<Value, ProviderError> {
    let mut input = Vec::new();
    for message in request.messages {
        match message {
            Message::Context(fragment) => input.push(json!({"role":"developer","content":[{"type":"input_text","text":fragment.text()}]})),
            Message::User(user) => {
                let mut content = Vec::new();
                for block in user.content {
                    match block {
                        Content::Text(text) => content.push(json!({"type":"input_text","text":text})),
                        Content::Image { media_type, data } => {
                            let encoded = STANDARD.encode(data);
                            content.push(json!({"type":"input_image","image_url":format!("data:{media_type};base64,{encoded}")}));
                        }
                        Content::Reasoning(_) | Content::Continuation { .. } => return Err(error(ErrorKind::Protocol, "invalid user content")),
                    }
                }
                input.push(json!({"role":"user","content":content}));
            }
            Message::Assistant { content, tool_calls } => {
                let mut text = Vec::new();
                let mut replayed_message = false;
                for block in content {
                    match block {
                        Content::Text(value) => text.push(json!({"type":"output_text","text":value})),
                        Content::Continuation { provider, data } if provider == PROVIDER => {
                            let item: Value = serde_json::from_slice(&data).map_err(|_| error(ErrorKind::Protocol, "invalid OpenAI continuation"))?;
                            match item["type"].as_str() {
                                Some("message") if item["role"] == "assistant" => replayed_message = true,
                                Some("reasoning") => {},
                                _ => return Err(error(ErrorKind::Protocol, "unsupported OpenAI continuation")),
                            }
                            input.push(item);
                        }
                        Content::Continuation { .. } | Content::Reasoning(_) | Content::Image { .. } => {},
                    }
                }
                if !replayed_message && !text.is_empty() {
                    input.push(json!({"type":"message","role":"assistant","content":text}));
                }
                for call in tool_calls {
                    input.push(json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":call.arguments}));
                }
            }
            Message::Tool(output) => input.push(json!({"type":"function_call_output","call_id":output.call_id,"output":output.text()})),
        }
    }
    let tools: Vec<_> = request.tools.into_iter().map(|tool| json!({"type":"function","name":tool.name,"description":tool.description,"parameters":tool.parameters,"strict":false})).collect();
    let mut body = json!({
        "model":request.model,"instructions":"","input":input,"tools":tools,
        "tool_choice":"auto","parallel_tool_calls":false,"store":false,"stream":true,
        "include":["reasoning.encrypted_content"],
    });
    if let Some(effort) = request.reasoning {
        body["reasoning"] = json!({"effort":effort,"summary":"auto"});
    }
    Ok(body)
}

pub(crate) fn decode(value: Value) -> Result<Vec<ModelEvent>, ProviderError> {
    let mut events = Vec::new();
    match value["type"].as_str() {
        Some("response.output_text.delta") => {
            events.push(ModelEvent::Text(field(&value, "delta")?))
        }
        Some("response.reasoning_summary_text.delta" | "response.reasoning_text.delta") => {
            events.push(ModelEvent::Reasoning(field(&value, "delta")?))
        }
        Some("response.output_item.done") => {
            let item = &value["item"];
            match item["type"].as_str() {
                Some("function_call") => events.push(ModelEvent::ToolCall(RawToolCall {
                    id: field(item, "call_id")?,
                    name: field(item, "name")?,
                    arguments: field(item, "arguments")?,
                })),
                Some("reasoning" | "message") => {
                    let data = serde_json::to_vec(item)
                        .map_err(|_| error(ErrorKind::Protocol, "cannot encode continuation"))?;
                    if data.len() > antex_core::MAX_STATE_BYTES {
                        return Err(error(
                            ErrorKind::Limit,
                            "OpenAI continuation exceeds its budget",
                        ));
                    }
                    events.push(ModelEvent::Continuation {
                        provider: PROVIDER.into(),
                        data: data.into(),
                    });
                }
                _ => return Err(error(ErrorKind::Protocol, "unsupported OpenAI output item")),
            }
        }
        Some("response.completed") => {
            if value["response"]["status"]
                .as_str()
                .is_some_and(|status| status != "completed")
            {
                return Err(error(
                    ErrorKind::Protocol,
                    "response did not complete successfully",
                ));
            }
            let usage = &value["response"]["usage"];
            events.push(ModelEvent::Finished(Usage {
                input_tokens: usage["input_tokens"].as_u64().unwrap_or_default(),
                output_tokens: usage["output_tokens"].as_u64().unwrap_or_default(),
                cached_input_tokens: usage["input_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or_default(),
            }));
        }
        Some("response.failed" | "response.incomplete" | "error") => {
            return Err(error(
                ErrorKind::Protocol,
                "OpenAI response failed or was incomplete",
            ));
        }
        Some(_) => {}
        None => {
            return Err(error(
                ErrorKind::Protocol,
                "OpenAI event is missing its type",
            ));
        }
    }
    Ok(events)
}

fn field(value: &Value, key: &str) -> Result<String, ProviderError> {
    value[key].as_str().map(str::to_owned).ok_or_else(|| {
        error(
            ErrorKind::Protocol,
            "OpenAI event is missing a required string",
        )
    })
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
