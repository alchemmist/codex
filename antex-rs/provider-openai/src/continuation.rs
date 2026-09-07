use antex_core::ErrorKind;
use antex_core::ProviderError;
use serde_json::Value;
use serde_json::json;

use crate::wire::error;

pub(crate) fn normalize(item: &Value) -> Result<Value, ProviderError> {
    let kind = item["type"].as_str().unwrap_or_default();
    let (key, text_kind) = match kind {
        "message" if item["role"] == "assistant" => ("content", "output_text"),
        "reasoning" => ("summary", "summary_text"),
        _ => return Err(error(ErrorKind::Protocol, "invalid continuation item type")),
    };
    let blocks = item[key]
        .as_array()
        .filter(|blocks| blocks.len() <= 64)
        .ok_or_else(|| error(ErrorKind::Protocol, "invalid continuation content"))?;
    let mut content = Vec::new();
    let mut size = 0;
    for block in blocks {
        if block["type"] != text_kind {
            return Err(error(
                ErrorKind::Protocol,
                "unsupported continuation content block",
            ));
        }
        let text = block["text"]
            .as_str()
            .ok_or_else(|| error(ErrorKind::Protocol, "invalid continuation text"))?;
        size += text.len();
        if size > antex_core::MAX_TEXT_BYTES {
            return Err(error(
                ErrorKind::Limit,
                "continuation text exceeds its byte budget",
            ));
        }
        content.push(json!({"type":text_kind,"text":text}));
    }
    let mut result = json!({"type":kind});
    result[key] = Value::Array(content);
    if let Some(id) = item["id"].as_str() {
        if id.len() > 128 {
            return Err(error(
                ErrorKind::Limit,
                "continuation identifier exceeds its byte budget",
            ));
        }
        result["id"] = json!(id);
    }
    if kind == "message" {
        result["role"] = json!("assistant");
        if let Some(phase) = item["phase"].as_str() {
            if !matches!(phase, "commentary" | "final_answer") {
                return Err(error(ErrorKind::Protocol, "unsupported assistant phase"));
            }
            result["phase"] = json!(phase);
        }
    } else if let Some(encrypted) = item["encrypted_content"].as_str() {
        if encrypted.len() > antex_core::MAX_STATE_BYTES {
            return Err(error(
                ErrorKind::Limit,
                "encrypted continuation exceeds its byte budget",
            ));
        }
        result["encrypted_content"] = json!(encrypted);
    }
    Ok(result)
}
