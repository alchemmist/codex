use std::time::Duration;

use antex_core::ErrorKind;
use antex_core::ModelEvent;
use antex_core::ModelStream;
use antex_core::ProviderError;
use futures::StreamExt;
use reqwest::Method;
use reqwest::Response;
use serde_json::Value;

use crate::auth::Auth;
use crate::auth::Refresh;
use crate::wire;
use crate::wire::COMPATIBILITY_REVISION;
use crate::wire::error;

pub(crate) async fn authorized(
    auth: &Auth,
    method: Method,
    url: &str,
    body: Option<&Value>,
) -> Result<Response, ProviderError> {
    let mut tokens = auth.credentials(Refresh::IfExpired).await?;
    for attempt in 0..2 {
        let mut request = auth
            .client
            .request(method.clone(), url)
            .bearer_auth(&tokens.access_token)
            .header("chatgpt-account-id", &tokens.account_id)
            .header("originator", "codex_cli_rs")
            .header("version", COMPATIBILITY_REVISION)
            .header(
                "user-agent",
                format!("codex_cli_rs/{COMPATIBILITY_REVISION} antex/0.0.0"),
            );
        if let Some(body) = body {
            request = request.json(body).header("accept", "text/event-stream");
        }
        let response = tokio::time::timeout(Duration::from_secs(30), request.send())
            .await
            .map_err(|_| error(ErrorKind::Transport, "OpenAI request timed out"))?
            .map_err(|_| error(ErrorKind::Transport, "OpenAI connection failed"))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
            tokens = auth
                .credentials(Refresh::Rejected(&tokens.access_token))
                .await?;
            continue;
        }
        if !response.status().is_success() {
            let kind = match response.status().as_u16() {
                401 | 403 => ErrorKind::Authentication,
                429 => ErrorKind::RateLimited,
                500..=599 => ErrorKind::Transport,
                _ => ErrorKind::Protocol,
            };
            return Err(ProviderError {
                kind,
                message: format!(
                    "OpenAI request failed with HTTP {}",
                    response.status().as_u16()
                ),
            });
        }
        return Ok(response);
    }
    Err(error(
        ErrorKind::Authentication,
        "OpenAI authentication was rejected",
    ))
}

pub(crate) fn response_stream(response: Response) -> ModelStream {
    Box::pin(async_stream::try_stream! {
        if let Some(quota) = crate::limits::headers(response.headers()) {
            yield ModelEvent::Quota(quota);
        }
        let mut stream = response.bytes_stream();
        let mut decoder = Decoder::default();
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(120), stream.next()).await
            .map_err(|_| error(ErrorKind::Transport, "OpenAI stream stalled"))?
        {
            let chunk = chunk.map_err(|_| error(ErrorKind::Transport, "OpenAI stream disconnected"))?;
            for byte in chunk {
                if let Some(value) = decoder.push(byte)? {
                    for event in wire::decode(value)? {
                        let finished = matches!(event, ModelEvent::Finished(_));
                        yield event;
                        if finished {
                            return;
                        }
                    }
                }
            }
        }
        Err(error(ErrorKind::Protocol, "OpenAI stream ended before completion"))?;
    })
}

#[derive(Default)]
struct Decoder {
    line: Vec<u8>,
    data: Vec<u8>,
}

impl Decoder {
    fn push(&mut self, byte: u8) -> Result<Option<Value>, ProviderError> {
        if self.line.len() + self.data.len() >= 1024 * 1024 {
            return Err(error(
                ErrorKind::Limit,
                "OpenAI event exceeds its byte budget",
            ));
        }
        if byte != b'\n' {
            self.line.push(byte);
            return Ok(None);
        }
        if self.line.last() == Some(&b'\r') {
            self.line.pop();
        }
        let line = std::mem::take(&mut self.line);
        if line.is_empty() {
            if self.data.is_empty() {
                return Ok(None);
            }
            let data = std::mem::take(&mut self.data);
            return serde_json::from_slice(&data)
                .map(Some)
                .map_err(|_| error(ErrorKind::Protocol, "invalid OpenAI event JSON"));
        }
        if let Some(data) = line.strip_prefix(b"data:") {
            if !self.data.is_empty() {
                self.data.push(b'\n');
            }
            self.data
                .extend_from_slice(data.strip_prefix(b" ").unwrap_or(data));
        }
        Ok(None)
    }
}
