use std::time::Duration;

use antex_core::ErrorKind;
use antex_core::ProviderError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::Digest;
use sha2::Sha256;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::OpenAiProvider;
use crate::auth::CLIENT_ID;
use crate::wire::error;

pub struct BrowserLogin {
    pub authorization_url: String,
    listener: TcpListener,
    verifier: String,
    state: String,
    redirect: String,
}

impl OpenAiProvider {
    pub async fn begin_browser_login(&self) -> Result<BrowserLogin, ProviderError> {
        let listener = match TcpListener::bind("127.0.0.1:1455").await {
            Ok(listener) => listener,
            Err(_) => TcpListener::bind("127.0.0.1:1457").await.map_err(|_| {
                error(
                    ErrorKind::Authentication,
                    "login callback ports are busy; use device login",
                )
            })?,
        };
        let port = listener
            .local_addr()
            .map_err(|_| error(ErrorKind::Authentication, "cannot inspect login callback"))?
            .port();
        let redirect = format!("http://localhost:{port}/auth/callback");
        let mut bytes = [0u8; 64];
        rand::rng().fill_bytes(&mut bytes);
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        rand::rng().fill_bytes(&mut bytes);
        let state = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url = Url::parse(&format!("{}/oauth/authorize", self.auth.issuer))
            .map_err(|_| error(ErrorKind::Authentication, "invalid OAuth issuer"))?;
        url.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", &redirect),
            ("scope", "openid profile email offline_access"),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("id_token_add_organizations", "true"),
            ("codex_cli_simplified_flow", "true"),
            ("originator", "codex_cli_rs"),
        ]);
        Ok(BrowserLogin {
            authorization_url: url.into(),
            listener,
            verifier,
            state,
            redirect,
        })
    }

    pub async fn finish_browser_login(
        &self,
        login: BrowserLogin,
        alias: String,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let complete = async {
            loop {
                let (mut socket, _) = login
                    .listener
                    .accept()
                    .await
                    .map_err(|_| error(ErrorKind::Authentication, "login callback failed"))?;
                let read = async {
                    let mut bytes = Vec::new();
                    while bytes.len() < 8192 {
                        let byte = socket.read_u8().await.map_err(|_| {
                            error(ErrorKind::Authentication, "login callback disconnected")
                        })?;
                        bytes.push(byte);
                        if bytes.ends_with(b"\r\n\r\n") {
                            return Ok(bytes);
                        }
                    }
                    Err(error(
                        ErrorKind::Authentication,
                        "login callback exceeds its size budget",
                    ))
                };
                let bytes = match tokio::time::timeout(Duration::from_secs(10), read).await {
                    Ok(Ok(bytes)) => bytes,
                    _ => continue,
                };
                let request = String::from_utf8(bytes).map_err(|_| {
                    error(ErrorKind::Authentication, "invalid login callback encoding")
                })?;
                let code = callback_code(&request, &login.state);
                let valid_callback = code.is_ok();
                let result = match code {
                    Ok(code) => {
                        match self
                            .exchange_code(&code, &login.verifier, &login.redirect)
                            .await
                        {
                            Ok(value) => self.auth.save_login(alias.clone(), value).await,
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                };
                let (status, body) = if result.is_ok() {
                    ("200 OK", "Signed in to Antex. You may close this tab.")
                } else {
                    ("400 Bad Request", "Login failed. Return to Antex.")
                };
                let length = body.len();
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}"
                );
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    socket.write_all(reply.as_bytes()),
                )
                .await;
                if valid_callback {
                    return result;
                }
            }
        };
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(error(ErrorKind::Cancelled, "login cancelled")),
            result = tokio::time::timeout(Duration::from_secs(900), complete) => result.map_err(|_| error(ErrorKind::Authentication, "browser login expired"))?,
        }
    }
}

fn callback_code(request: &str, state: &str) -> Result<String, ProviderError> {
    let mut parts = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = parts.next();
    let target = parts.next().unwrap_or_default();
    let url = Url::parse(&format!("http://localhost{target}"))
        .map_err(|_| error(ErrorKind::Authentication, "invalid callback URL"))?;
    let query: Vec<_> = url.query_pairs().collect();
    let states: Vec<_> = query.iter().filter(|(key, _)| key == "state").collect();
    let codes: Vec<_> = query.iter().filter(|(key, _)| key == "code").collect();
    if method != Some("GET")
        || url.path() != "/auth/callback"
        || states.len() != 1
        || states[0].1 != state
        || codes.len() != 1
        || codes[0].1.is_empty()
        || codes[0].1.len() > 1024
    {
        return Err(error(
            ErrorKind::Authentication,
            "invalid OAuth state or authorization code",
        ));
    }
    Ok(codes[0].1.to_string())
}
