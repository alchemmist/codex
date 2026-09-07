use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use antex_core::ErrorKind;
use antex_core::ProviderError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Client;
use serde_json::Value;

use crate::storage::Store;
use crate::storage::Tokens;
use crate::wire::error;

pub(crate) const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(crate) const ISSUER: &str = "https://auth.openai.com";

pub(crate) struct Auth {
    pub client: Client,
    pub issuer: String,
    pub store: Store,
}

pub(crate) enum Refresh<'a> {
    IfExpired,
    Rejected(&'a str),
}

impl Auth {
    pub fn new(home: &Path, client: Client) -> Self {
        Self {
            client,
            issuer: ISSUER.into(),
            store: Store::new(home),
        }
    }

    pub async fn credentials(&self, refresh: Refresh<'_>) -> Result<Tokens, ProviderError> {
        let _lock = self.store.lock().await?;
        let mut accounts = self.store.load()?;
        let alias = accounts
            .active
            .clone()
            .ok_or_else(|| error(ErrorKind::Authentication, "sign in with antex login"))?;
        let tokens = accounts
            .entries
            .get_mut(&alias)
            .ok_or_else(|| error(ErrorKind::Authentication, "active account is missing"))?;
        let needs_refresh = match refresh {
            Refresh::IfExpired => tokens.expires_at <= now().saturating_add(60),
            Refresh::Rejected(previous) => tokens.access_token == previous,
        };
        if needs_refresh {
            let response = self.client.post(format!("{}/oauth/token", self.issuer))
                .timeout(Duration::from_secs(30))
                .json(&serde_json::json!({"client_id":CLIENT_ID,"grant_type":"refresh_token","refresh_token":tokens.refresh_token}))
                .send().await.map_err(|_| error(ErrorKind::Transport, "token refresh transport failed"))?;
            let value = token_response(response).await?;
            let refreshed = tokens_from_response(&value, Some(tokens))?;
            if refreshed.account_id != tokens.account_id {
                return Err(error(
                    ErrorKind::Authentication,
                    "token refresh changed the active account",
                ));
            }
            *tokens = refreshed;
            self.store.save(&accounts)?;
        }
        Ok(accounts.entries.remove(&alias).unwrap())
    }

    pub async fn save_login(&self, alias: String, value: Value) -> Result<(), ProviderError> {
        if alias.is_empty() || alias.len() > 128 || alias.chars().any(char::is_control) {
            return Err(error(ErrorKind::Authentication, "invalid account name"));
        }
        let tokens = tokens_from_response(&value, /*previous*/ None)?;
        let _lock = self.store.lock().await?;
        let mut accounts = self.store.load()?;
        if accounts.entries.len() >= 32 && !accounts.entries.contains_key(&alias) {
            return Err(error(ErrorKind::Limit, "too many saved accounts"));
        }
        accounts.entries.insert(alias.clone(), tokens);
        accounts.active = Some(alias);
        self.store.save(&accounts)
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) async fn token_response(response: reqwest::Response) -> Result<Value, ProviderError> {
    if !response.status().is_success() {
        return Err(error(
            ErrorKind::Authentication,
            "OAuth request rejected; sign in again",
        ));
    }
    let bytes = crate::body::bounded_body(response, 1024 * 1024).await?;
    serde_json::from_slice(&bytes)
        .map_err(|_| error(ErrorKind::Authentication, "invalid OAuth response"))
}

fn tokens_from_response(value: &Value, previous: Option<&Tokens>) -> Result<Tokens, ProviderError> {
    let access_token = value["access_token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            error(
                ErrorKind::Authentication,
                "OAuth response has no access token",
            )
        })?;
    let claims = claims(access_token)?;
    let account_id = claims["https://api.openai.com/auth"]["chatgpt_account_id"]
        .as_str()
        .or_else(|| previous.map(|tokens| tokens.account_id.as_str()))
        .ok_or_else(|| {
            error(
                ErrorKind::Authentication,
                "OAuth response has no ChatGPT account",
            )
        })?;
    let refresh_token = value["refresh_token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .or_else(|| previous.map(|tokens| tokens.refresh_token.as_str()))
        .ok_or_else(|| {
            error(
                ErrorKind::Authentication,
                "OAuth response has no refresh token",
            )
        })?;
    let expires_at = claims["exp"]
        .as_u64()
        .or_else(|| {
            value["expires_in"]
                .as_u64()
                .map(|seconds| now().saturating_add(seconds))
        })
        .ok_or_else(|| {
            error(
                ErrorKind::Authentication,
                "OAuth response has no token expiration",
            )
        })?;
    if account_id.is_empty()
        || account_id.len() > 128
        || access_token.len() > 64 * 1024
        || refresh_token.len() > 64 * 1024
    {
        return Err(error(
            ErrorKind::Authentication,
            "OAuth credentials exceed their size budget",
        ));
    }
    Ok(Tokens {
        access_token: access_token.into(),
        refresh_token: refresh_token.into(),
        account_id: account_id.into(),
        expires_at,
    })
}

fn claims(token: &str) -> Result<Value, ProviderError> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| error(ErrorKind::Authentication, "invalid access token"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| error(ErrorKind::Authentication, "invalid access token payload"))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| error(ErrorKind::Authentication, "invalid access token claims"))
}
