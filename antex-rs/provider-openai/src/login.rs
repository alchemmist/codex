use std::time::Duration;

use antex_core::ErrorKind;
use antex_core::ProviderError;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::OpenAiProvider;
use crate::auth::CLIENT_ID;
use crate::auth::token_response;
use crate::wire::error;

pub struct DeviceLogin {
    pub verification_url: String,
    pub user_code: String,
    device_auth_id: String,
    interval: Duration,
}

impl OpenAiProvider {
    pub async fn begin_device_login(&self) -> Result<DeviceLogin, ProviderError> {
        let response = self
            .auth
            .client
            .post(format!(
                "{}/api/accounts/deviceauth/usercode",
                self.auth.issuer
            ))
            .timeout(Duration::from_secs(30))
            .json(&json!({"client_id":CLIENT_ID}))
            .send()
            .await
            .map_err(|_| error(ErrorKind::Transport, "device login request failed"))?;
        let value = token_response(response).await?;
        let device_auth_id = text(&value, "device_auth_id")?;
        let user_code = value["user_code"]
            .as_str()
            .or_else(|| value["usercode"].as_str())
            .filter(|code| !code.is_empty() && code.len() <= 128)
            .ok_or_else(|| {
                error(
                    ErrorKind::Authentication,
                    "device login returned an invalid user code",
                )
            })?
            .into();
        let interval = value["interval"]
            .as_u64()
            .or_else(|| {
                value["interval"]
                    .as_str()
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(5)
            .clamp(1, 60);
        Ok(DeviceLogin {
            verification_url: format!("{}/codex/device", self.auth.issuer),
            user_code,
            device_auth_id,
            interval: Duration::from_secs(interval),
        })
    }

    pub async fn finish_device_login(
        &self,
        login: DeviceLogin,
        alias: String,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let complete = async {
            loop {
                let response = self
                    .auth
                    .client
                    .post(format!(
                        "{}/api/accounts/deviceauth/token",
                        self.auth.issuer
                    ))
                    .timeout(Duration::from_secs(30))
                    .json(
                        &json!({"device_auth_id":login.device_auth_id,"user_code":login.user_code}),
                    )
                    .send()
                    .await
                    .map_err(|_| error(ErrorKind::Transport, "device login polling failed"))?;
                if matches!(response.status().as_u16(), 403 | 404) {
                    tokio::time::sleep(login.interval).await;
                    continue;
                }
                let value = token_response(response).await?;
                let code = text(&value, "authorization_code")?;
                let verifier = text(&value, "code_verifier")?;
                let redirect = format!("{}/deviceauth/callback", self.auth.issuer);
                let value = self.exchange_code(&code, &verifier, &redirect).await?;
                return self.auth.save_login(alias, value).await;
            }
        };
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(error(ErrorKind::Cancelled, "login cancelled")),
            result = tokio::time::timeout(Duration::from_secs(900), complete) => result.map_err(|_| error(ErrorKind::Authentication, "device login expired"))?,
        }
    }

    pub(crate) async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect: &str,
    ) -> Result<Value, ProviderError> {
        let response = self
            .auth
            .client
            .post(format!("{}/oauth/token", self.auth.issuer))
            .timeout(Duration::from_secs(30))
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", CLIENT_ID),
                ("code", code),
                ("code_verifier", verifier),
                ("redirect_uri", redirect),
            ])
            .send()
            .await
            .map_err(|_| error(ErrorKind::Transport, "OAuth code exchange failed"))?;
        token_response(response).await
    }
}

fn text(value: &Value, field: &str) -> Result<String, ProviderError> {
    value[field]
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= 1024)
        .map(str::to_owned)
        .ok_or_else(|| error(ErrorKind::Authentication, "invalid device login response"))
}
