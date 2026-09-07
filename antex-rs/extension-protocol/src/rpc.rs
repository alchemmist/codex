use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::MAX_FRAME_BYTES;
use crate::ProtocolError;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Version {
    #[serde(rename = "2.0")]
    V2,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Method {
    #[serde(rename = "initialize")]
    Initialize,
    #[serde(rename = "tool/call")]
    ToolCall,
    #[serde(rename = "command/run")]
    CommandRun,
    #[serde(rename = "event/notify")]
    EventNotify,
    #[serde(rename = "shutdown")]
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub jsonrpc: Version,
    pub id: String,
    pub method: Method,
    pub params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Success(Success),
    Failure(Failure),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Success {
    pub jsonrpc: Version,
    pub id: String,
    pub result: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub jsonrpc: Version,
    pub id: String,
    pub error: RpcError,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| ProtocolError::Encoding)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::Limit("frame"));
    }
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtocolError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::Limit("frame"));
    }
    serde_json::from_slice(bytes).map_err(|_| ProtocolError::Encoding)
}

impl Response {
    pub fn result(self, expected_id: &str) -> Result<Value, ProtocolError> {
        match self {
            Self::Success(response) if response.id == expected_id => Ok(response.result),
            Self::Failure(response) if response.id == expected_id => {
                if response.error.message.len() > 512 {
                    return Err(ProtocolError::Limit("error message"));
                }
                Err(ProtocolError::Remote {
                    code: response.error.code,
                    message: response.error.message,
                })
            }
            Self::Success(_) | Self::Failure(_) => Err(ProtocolError::MismatchedId),
        }
    }
}
