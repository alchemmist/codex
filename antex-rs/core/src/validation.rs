use crate::Content;
use crate::ErrorKind;
use crate::MAX_IMAGE_BYTES;
use crate::MAX_STATE_BYTES;
use crate::MAX_TEXT_BYTES;
use crate::MAX_TOOLS;
use crate::MAX_TRANSCRIPT_BYTES;
use crate::Message;
use crate::ProviderError;
use crate::ToolScope;
use crate::UserInput;

pub(crate) fn error(kind: ErrorKind, message: &str) -> ProviderError {
    ProviderError {
        kind,
        message: message.into(),
    }
}

fn limit(message: &str) -> ProviderError {
    ProviderError {
        kind: ErrorKind::Limit,
        message: message.into(),
    }
}

pub(crate) fn content_size(content: &[Content]) -> Result<usize, ProviderError> {
    if content.len() > 64 {
        return Err(limit("too many content blocks"));
    }
    let mut size = 0;
    for block in content {
        let bytes = match block {
            Content::Text(text) | Content::Reasoning(text) => {
                if text.len() > MAX_TEXT_BYTES {
                    return Err(limit("text block exceeds its byte budget"));
                }
                text.len()
            }
            Content::Image { media_type, data } => {
                if media_type.len() > 128 || data.len() > MAX_IMAGE_BYTES {
                    return Err(limit("image block exceeds its byte budget"));
                }
                media_type.len() + data.len()
            }
            Content::Continuation { provider, data } => {
                if provider.len() > 128 || data.len() > MAX_STATE_BYTES {
                    return Err(limit("continuation block exceeds its byte budget"));
                }
                provider.len() + data.len()
            }
        };
        size += bytes;
    }
    Ok(size)
}

pub(crate) fn user(input: &UserInput) -> Result<usize, ProviderError> {
    if input
        .content
        .iter()
        .any(|item| matches!(item, Content::Reasoning(_) | Content::Continuation { .. }))
    {
        return Err(limit(
            "user input cannot supply provider continuation or reasoning blocks",
        ));
    }
    if let ToolScope::Named(name) = &input.tool_scope
        && name.len() > 128
    {
        return Err(limit("tool scope exceeds its byte budget"));
    }
    let size = content_size(&input.content)?;
    if size > MAX_TRANSCRIPT_BYTES {
        return Err(limit("user input exceeds its byte budget"));
    }
    Ok(size.max(1))
}

pub(crate) fn messages(messages: &[Message]) -> Result<(), ProviderError> {
    if messages.len() > 4096 {
        return Err(limit("too many messages in the active transcript"));
    }
    if context_size(messages)? > MAX_TRANSCRIPT_BYTES {
        return Err(limit("active transcript exceeds its byte budget"));
    }
    Ok(())
}

pub(crate) fn history(messages: &[Message]) -> Result<(), ProviderError> {
    context_size(messages).map(|_| ())
}

pub fn context_size(messages: &[Message]) -> Result<usize, ProviderError> {
    if messages.len() > 16_384 {
        return Err(limit("too many messages in the working history"));
    }
    let mut size = 0;
    for message in messages {
        let bytes = match message {
            Message::Context(fragment) => fragment.text().len(),
            Message::User(input) => user(input)?,
            Message::Assistant {
                content,
                tool_calls,
            } => {
                if tool_calls.len() > MAX_TOOLS {
                    return Err(limit("too many tool calls in one assistant message"));
                }
                let mut bytes = content_size(content)?;
                for call in tool_calls {
                    if call.id.is_empty()
                        || call.id.len() > 128
                        || call.name.len() > 64
                        || call.arguments.len() > MAX_TEXT_BYTES
                    {
                        return Err(limit("tool call exceeds its identifier or argument budget"));
                    }
                    bytes += call.id.len() + call.name.len() + call.arguments.len();
                }
                bytes
            }
            Message::Tool(output) => {
                if output.call_id.is_empty() || output.call_id.len() > 128 {
                    return Err(limit("tool result has an invalid call identifier"));
                }
                output.text().len()
                    + output.call_id.len()
                    + output
                        .image()
                        .map(|image| content_size(std::slice::from_ref(image)))
                        .transpose()?
                        .unwrap_or_default()
            }
        };
        if bytes > MAX_TRANSCRIPT_BYTES {
            return Err(limit("message exceeds its byte budget"));
        }
        size += bytes;
        if size > crate::MAX_HISTORY_BYTES {
            return Err(limit("working history exceeds its byte budget"));
        }
    }
    Ok(size)
}
