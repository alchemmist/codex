use std::io;

use antex_core::Content;
use antex_core::ContextFragment;
use antex_core::ContextKind;
use antex_core::MAX_IMAGE_BYTES;
use antex_core::MAX_STATE_BYTES;
use antex_core::MAX_TEXT_BYTES;
use antex_core::MAX_TOOLS;
use antex_core::Message;
use antex_core::RawToolCall;
use antex_core::ToolOutcome;
use antex_core::ToolOutput;
use antex_core::ToolScope;
use antex_core::UserInput;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use serde_json::json;

pub(crate) fn encode(message: &Message) -> Value {
    match message {
        Message::Context(fragment) => {
            json!({"type":"context","kind":match fragment.kind() {ContextKind::System=>"system",ContextKind::Project=>"project",ContextKind::Skill=>"skill",ContextKind::Summary=>"summary"},"text":fragment.text()})
        }
        Message::User(input) => {
            json!({"type":"user","content":input.content.iter().map(content).collect::<Vec<_>>(),"tool_scope":match &input.tool_scope {ToolScope::Default=>None,ToolScope::Named(name)=>Some(name)}})
        }
        Message::Assistant {
            content: blocks,
            tool_calls,
        } => {
            json!({"type":"assistant","content":blocks.iter().map(content).collect::<Vec<_>>(),"tool_calls":tool_calls.iter().map(|call| json!({"id":call.id,"name":call.name,"arguments":call.arguments})).collect::<Vec<_>>()})
        }
        Message::Tool(output) => {
            json!({"type":"tool_result","call_id":output.call_id,"outcome":match output.outcome {ToolOutcome::Success=>"success",ToolOutcome::Failure=>"failure",ToolOutcome::Cancelled=>"cancelled"},"text":output.text()})
        }
    }
}

pub(crate) fn decode(value: &Value) -> io::Result<Message> {
    match text(value, "type")? {
        "context" => {
            let kind = match text(value, "kind")? {
                "system" => ContextKind::System,
                "project" => ContextKind::Project,
                "skill" => ContextKind::Skill,
                "summary" => ContextKind::Summary,
                _ => return Err(invalid()),
            };
            Ok(Message::Context(
                ContextFragment::new(kind, text(value, "text")?.into()).map_err(|_| invalid())?,
            ))
        }
        "user" => {
            let tool_scope = match value.get("tool_scope") {
                None | Some(Value::Null) => ToolScope::Default,
                Some(Value::String(name)) if name.len() <= 128 => ToolScope::Named(name.clone()),
                Some(_) => return Err(invalid()),
            };
            let content = contents(value)?;
            if content
                .iter()
                .any(|block| matches!(block, Content::Reasoning(_) | Content::Continuation { .. }))
            {
                return Err(invalid());
            }
            Ok(Message::User(UserInput {
                content,
                tool_scope,
            }))
        }
        "assistant" => {
            let calls = value["tool_calls"]
                .as_array()
                .filter(|calls| calls.len() <= MAX_TOOLS)
                .ok_or_else(invalid)?;
            let tool_calls = calls
                .iter()
                .map(|call| {
                    let id = text(call, "id")?;
                    let name = text(call, "name")?;
                    let arguments = text(call, "arguments")?;
                    if id.is_empty()
                        || id.len() > 128
                        || name.len() > 64
                        || arguments.len() > MAX_TEXT_BYTES
                    {
                        return Err(invalid());
                    }
                    Ok(RawToolCall {
                        id: id.into(),
                        name: name.into(),
                        arguments: arguments.into(),
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            Ok(Message::Assistant {
                content: contents(value)?,
                tool_calls,
            })
        }
        "tool_result" => {
            let id = text(value, "call_id")?;
            let output = text(value, "text")?;
            if id.is_empty() || id.len() > 128 || output.len() > MAX_TEXT_BYTES {
                return Err(invalid());
            }
            let outcome = match text(value, "outcome")? {
                "success" => ToolOutcome::Success,
                "failure" => ToolOutcome::Failure,
                "cancelled" => ToolOutcome::Cancelled,
                _ => return Err(invalid()),
            };
            Ok(Message::Tool(ToolOutput::new(
                id.into(),
                outcome,
                output.into(),
            )))
        }
        _ => Err(invalid()),
    }
}

fn content(block: &Content) -> Value {
    match block {
        Content::Text(text) => json!({"type":"text","text":text}),
        Content::Reasoning(text) => json!({"type":"reasoning","text":text}),
        Content::Image { media_type, data } => {
            json!({"type":"image","media_type":media_type,"data":STANDARD.encode(data)})
        }
        Content::Continuation { provider, data } => {
            json!({"type":"continuation","provider":provider,"data":STANDARD.encode(data)})
        }
    }
}

fn contents(value: &Value) -> io::Result<Vec<Content>> {
    let blocks = value["content"]
        .as_array()
        .filter(|blocks| blocks.len() <= 64)
        .ok_or_else(invalid)?;
    blocks
        .iter()
        .map(|block| match text(block, "type")? {
            "text" | "reasoning" => {
                let content = text(block, "text")?;
                if content.len() > MAX_TEXT_BYTES {
                    return Err(invalid());
                }
                Ok(if block["type"] == "text" {
                    Content::Text(content.into())
                } else {
                    Content::Reasoning(content.into())
                })
            }
            "image" => {
                let media_type = text(block, "media_type")?;
                if media_type.len() > 128 {
                    return Err(invalid());
                }
                Ok(Content::Image {
                    media_type: media_type.into(),
                    data: bytes(block, MAX_IMAGE_BYTES)?.into(),
                })
            }
            "continuation" => {
                let provider = text(block, "provider")?;
                if provider.len() > 128 {
                    return Err(invalid());
                }
                Ok(Content::Continuation {
                    provider: provider.into(),
                    data: bytes(block, MAX_STATE_BYTES)?.into(),
                })
            }
            _ => Err(invalid()),
        })
        .collect()
}

fn bytes(value: &Value, limit: usize) -> io::Result<Vec<u8>> {
    let encoded = text(value, "data")?;
    if encoded.len() > limit.div_ceil(3) * 4 {
        return Err(invalid());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| invalid())?;
    if bytes.len() > limit {
        return Err(invalid());
    }
    Ok(bytes)
}

fn text<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value[key].as_str().ok_or_else(invalid)
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid or oversized session message",
    )
}
