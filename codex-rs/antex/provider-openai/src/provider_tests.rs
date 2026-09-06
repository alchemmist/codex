use antex_core::ErrorKind;
use antex_core::ModelEvent;
use antex_core::ModelInfo;
use antex_core::ModelProvider;
use antex_core::ModelRequest;
use antex_core::Usage;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

use crate::OpenAiProvider;
use crate::auth::now;

pub(super) fn token(expiration: u64, marker: &str) -> String {
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({"exp":expiration,"marker":marker,"https://api.openai.com/auth":{"chatgpt_account_id":"test-account"}})).unwrap());
    format!("e30.{payload}.signature")
}

pub(super) async fn provider(server: &MockServer, home: &std::path::Path) -> OpenAiProvider {
    let mut provider = OpenAiProvider::new(home).unwrap();
    provider.api_base = server.uri();
    provider.auth.issuer = server.uri();
    provider
        .auth
        .save_login(
            "test".into(),
            json!({"access_token":token(now()+3600,"initial"),"refresh_token":"fake-refresh"}),
        )
        .await
        .unwrap();
    provider
}

fn request() -> ModelRequest {
    ModelRequest {
        model: "fake".into(),
        reasoning: None,
        messages: Vec::new(),
        tools: Vec::new(),
    }
}

fn sse(events: Vec<Value>) -> String {
    events
        .into_iter()
        .map(|event| format!("data: {event}\r\n\r\n"))
        .collect()
}

#[tokio::test]
async fn streams_deltas_and_completion_with_private_compatibility_headers() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(header("version", "0.153.4"))
        .and(header("chatgpt-account-id", "test-account"))
        .and(body_partial_json(
            json!({"model":"fake","store":false,"stream":true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse(vec![
            json!({"type":"response.output_text.delta","delta":"hello"}),
            json!({"type":"response.completed","response":{"status":"completed"}}),
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let events: Vec<_> = provider.stream(request()).await.unwrap().collect().await;
    assert_eq!(
        events,
        vec![
            Ok(ModelEvent::Text("hello".into())),
            Ok(ModelEvent::Finished(Usage::default()))
        ]
    );
}

#[tokio::test]
async fn unauthorized_request_refreshes_once_and_persists_rotated_credentials() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    let old = provider.auth.store.load().unwrap().entries["test"]
        .access_token
        .clone();
    let new = token(now() + 7200, "rotated");
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(header("authorization", format!("Bearer {old}")))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_partial_json(
            json!({"grant_type":"refresh_token","refresh_token":"fake-refresh"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token":new,"refresh_token":"rotated-refresh"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(header("authorization", format!("Bearer {new}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse(vec![
            json!({"type":"response.completed","response":{"status":"completed"}}),
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let events: Vec<_> = provider.stream(request()).await.unwrap().collect().await;
    assert_eq!(events, vec![Ok(ModelEvent::Finished(Usage::default()))]);
    let saved = provider.auth.store.load().unwrap();
    assert_eq!(
        (
            &*saved.entries["test"].access_token,
            &*saved.entries["test"].refresh_token
        ),
        (&*new, "rotated-refresh")
    );
}

#[tokio::test]
async fn catalog_maps_only_provider_neutral_metadata() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(query_param("client_version", "0.153.4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models":[{
            "slug":"fake","display_name":"Fake Model","context_window":32000,
            "input_modalities":["text","image"],"supported_reasoning_levels":[{"effort":"high"}],
            "enterprise_policy":"ignored"
        }]})))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        provider.models().await.unwrap(),
        vec![ModelInfo {
            id: "fake".into(),
            display_name: "Fake Model".into(),
            context_window: 32000,
            reasoning_levels: vec!["high".into()],
            accepts_images: true
        }]
    );
}

#[tokio::test]
async fn disconnect_is_an_error_not_a_successful_empty_turn() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse(vec![
            json!({"type":"response.output_text.delta","delta":"partial"}),
        ])))
        .mount(&server)
        .await;
    let events: Vec<_> = provider.stream(request()).await.unwrap().collect().await;
    assert_eq!(
        events.first(),
        Some(&Ok(ModelEvent::Text("partial".into())))
    );
    assert_eq!(
        events.last().unwrap().as_ref().unwrap_err().kind,
        ErrorKind::Protocol
    );
}

#[tokio::test]
async fn account_selection_and_logout_do_not_touch_other_accounts() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    provider
        .auth
        .save_login(
            "second".into(),
            json!({"access_token":token(now()+3600,"second"),"refresh_token":"second-refresh"}),
        )
        .await
        .unwrap();
    provider.select_account("test").await.unwrap();
    provider.logout("second").await.unwrap();
    assert_eq!(provider.accounts().await.unwrap(), vec!["test"]);
    assert_eq!(
        provider.auth.store.load().unwrap().active,
        Some("test".into())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(home.path().join("providers/openai/auth.json"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
    }
}
