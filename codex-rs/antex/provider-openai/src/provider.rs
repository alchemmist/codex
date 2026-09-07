use std::path::Path;
use std::time::Duration;

use antex_core::ErrorKind;
use antex_core::ModelInfo;
use antex_core::ModelProvider;
use antex_core::ModelRequest;
use antex_core::ModelStream;
use antex_core::ProviderError;
use reqwest::Client;
use reqwest::Method;
use serde_json::Value;

use crate::auth::Auth;
use crate::http;
use crate::wire;
use crate::wire::COMPATIBILITY_REVISION;
use crate::wire::error;

pub struct OpenAiProvider {
    pub(crate) auth: Auth,
    pub(crate) api_base: String,
}

impl OpenAiProvider {
    pub fn new(home: &Path) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| error(ErrorKind::Transport, "cannot create OpenAI HTTP client"))?;
        Ok(Self {
            auth: Auth::new(home, client),
            api_base: "https://chatgpt.com/backend-api/codex".into(),
        })
    }

    pub async fn accounts(&self) -> Result<Vec<String>, ProviderError> {
        Ok(self.auth.store.load()?.entries.into_keys().collect())
    }

    pub async fn select_account(&self, name: &str) -> Result<(), ProviderError> {
        let _lock = self.auth.store.lock().await?;
        let mut accounts = self.auth.store.load()?;
        if !accounts.entries.contains_key(name) {
            return Err(error(ErrorKind::Authentication, "account not found"));
        }
        accounts.active = Some(name.into());
        self.auth.store.save(&accounts)
    }

    pub async fn logout(&self, name: &str) -> Result<(), ProviderError> {
        let _lock = self.auth.store.lock().await?;
        let mut accounts = self.auth.store.load()?;
        accounts.entries.remove(name);
        if accounts.active.as_deref() == Some(name) {
            accounts.active = None;
        }
        self.auth.store.save(&accounts)
    }
}

impl ModelProvider for OpenAiProvider {
    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!(
            "{}/models?client_version={COMPATIBILITY_REVISION}",
            self.api_base
        );
        let response = http::authorized(&self.auth, Method::GET, &url, /*body*/ None).await?;
        let bytes = crate::body::bounded_body(response, 1024 * 1024).await?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| error(ErrorKind::Protocol, "invalid model catalog"))?;
        let models = value["models"]
            .as_array()
            .filter(|models| models.len() <= 256)
            .ok_or_else(|| error(ErrorKind::Protocol, "invalid model catalog size"))?;
        models
            .iter()
            .filter(|model| model["visibility"] != "hide")
            .map(|model| {
                let id = model["slug"]
                    .as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 128)
                    .ok_or_else(|| error(ErrorKind::Protocol, "invalid model identifier"))?;
                let display_name = model["display_name"]
                    .as_str()
                    .filter(|name| name.len() <= 256)
                    .unwrap_or(id);
                let reasoning_levels = model["supported_reasoning_levels"]
                    .as_array()
                    .map(|levels| {
                        levels
                            .iter()
                            .take(32)
                            .filter_map(|level| {
                                level["effort"]
                                    .as_str()
                                    .filter(|effort| effort.len() <= 64)
                                    .map(str::to_owned)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let accepts_images = model["input_modalities"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item == "image"));
                Ok(ModelInfo {
                    id: id.into(),
                    display_name: display_name.into(),
                    context_window: model["context_window"]
                        .as_u64()
                        .or_else(|| model["max_context_window"].as_u64())
                        .unwrap_or(32_000),
                    reasoning_levels,
                    accepts_images,
                })
            })
            .collect()
    }

    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let body = wire::encode(request)?;
        let url = format!("{}/responses", self.api_base);
        let response = http::authorized(&self.auth, Method::POST, &url, Some(&body)).await?;
        Ok(http::response_stream(response))
    }
}
