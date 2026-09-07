use std::time::Duration;

use antex_core::ErrorKind;
use antex_core::ProviderError;
use futures::StreamExt;
use reqwest::Response;

use crate::wire::error;

pub(crate) async fn bounded_body(
    response: Response,
    limit: usize,
) -> Result<Vec<u8>, ProviderError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(30), stream.next())
        .await
        .map_err(|_| error(ErrorKind::Transport, "OpenAI body read timed out"))?
    {
        let chunk = chunk.map_err(|_| error(ErrorKind::Transport, "OpenAI body read failed"))?;
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(error(
                ErrorKind::Limit,
                "OpenAI response exceeds its byte budget",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
