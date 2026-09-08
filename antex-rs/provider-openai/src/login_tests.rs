use antex_core::ErrorKind;
use antex_core::ModelProvider;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::body_string_contains;
use wiremock::matchers::method;
use wiremock::matchers::path;

use crate::OpenAiProvider;
use crate::auth::now;
use crate::tests::provider;
use crate::tests::token;

#[tokio::test]
async fn device_login_exchanges_a_code_and_persists_an_independent_account() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    Mock::given(method("POST"))
        .and(path("/api/accounts/deviceauth/usercode"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"device_auth_id":"device","user_code":"ABC-123","interval":"1"}),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/accounts/deviceauth/token"))
        .and(body_partial_json(
            json!({"device_auth_id":"device","user_code":"ABC-123"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"authorization_code":"code","code_verifier":"verifier"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code_verifier=verifier"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token":token(now()+3600,"device"),"refresh_token":"device-refresh"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let login = provider.begin_device_login().await.unwrap();
    assert_eq!(
        (&*login.verification_url, &*login.user_code),
        (&*format!("{}/codex/device", server.uri()), "ABC-123")
    );
    provider
        .finish_device_login(login, "device".into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(provider.accounts().await.unwrap(), vec!["device", "test"]);
    assert_eq!(
        provider.auth.store.load().unwrap().active,
        Some("device".into())
    );
}

#[tokio::test]
async fn cancelled_device_login_does_not_poll_or_change_active_credentials() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    Mock::given(path("/api/accounts/deviceauth/usercode"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"device_auth_id":"device","user_code":"ABC","interval":1})),
        )
        .mount(&server)
        .await;
    let login = provider.begin_device_login().await.unwrap();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        provider
            .finish_device_login(login, "cancelled".into(), cancellation)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Cancelled
    );
    assert_eq!(provider.accounts().await.unwrap(), vec!["test"]);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn browser_login_rejects_wrong_state_before_exchanging_the_valid_code() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let provider = provider(&server, home.path()).await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("code=valid-code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token":token(now()+3600,"browser"),"refresh_token":"browser-refresh"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let login = provider.begin_browser_login().await.unwrap();
    let url = url::Url::parse(&login.authorization_url).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["code_challenge_method"], "S256");
    let callback = query["redirect_uri"].clone();
    let state = query["state"].clone();
    let requests = async {
        let client = reqwest::Client::new();
        let rejected = client
            .get(&callback)
            .query(&[("state", "wrong"), ("code", "attacker-code")])
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status().as_u16(), 400);
        let accepted = client
            .get(&callback)
            .query(&[("state", state.as_str()), ("code", "valid-code")])
            .send()
            .await
            .unwrap();
        assert_eq!(
            (accepted.status().as_u16(), accepted.text().await.unwrap()),
            (200, "Signed in to Antex. You may close this tab.".into())
        );
    };
    let (result, ()) = tokio::join!(
        provider.finish_browser_login(login, "browser".into(), CancellationToken::new()),
        requests
    );
    result.unwrap();
    assert_eq!(
        provider.auth.store.load().unwrap().active,
        Some("browser".into())
    );
}

#[cfg(unix)]
#[tokio::test]
async fn credential_store_rejects_symlinked_parent_without_touching_its_target() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), home.path().join("providers")).unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    assert_eq!(
        provider.models().await.unwrap_err().kind,
        ErrorKind::Authentication
    );
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn cancelling_a_busy_credential_lock_does_not_stall_runtime_shutdown() {
    let home = tempfile::tempdir().unwrap();
    let store = crate::storage::Store::new(home.path());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let held = runtime.block_on(store.lock()).unwrap();
    let second = crate::storage::Store::new(home.path());
    let (finished, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(50), second.lock())
                    .await
                    .is_err()
            );
        });
        drop(runtime);
        finished.send(()).unwrap();
    });
    let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
    drop(held);
    worker.join().unwrap();
    assert!(result.is_ok());
}
