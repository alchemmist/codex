use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use antex_core::ModelProvider;
use pretty_assertions::assert_eq;
use serde_json::json;

#[tokio::test]
async fn provider_uses_system_trust_for_https_catalogs() {
    if let Ok(endpoint) = std::env::var("ANTEX_TLS_FIXTURE") {
        let home = tempfile::tempdir().unwrap();
        let mut provider = crate::OpenAiProvider::new(home.path()).unwrap();
        provider.api_base = endpoint;
        provider.auth.save_login("fixture".into(), json!({"access_token":crate::tests::token(crate::auth::now()+3600,"tls"),"refresh_token":"fake-refresh"})).await.unwrap();
        let result = provider.models().await;
        if std::env::var_os("ANTEX_TLS_EXPECT_REJECT").is_some() {
            assert_eq!(result.unwrap_err().kind, antex_core::ErrorKind::Transport);
        } else {
            assert_eq!(result.unwrap(), Vec::new());
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let run = |arguments: &[&str]| {
        let output = Command::new("openssl")
            .args(arguments)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "certificate fixture generation failed"
        );
    };
    run(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        "ca.key",
        "-out",
        "ca.pem",
        "-subj",
        "/CN=Antex fixture CA",
        "-days",
        "1",
    ]);
    run(&[
        "req",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        "server.key",
        "-out",
        "server.csr",
        "-subj",
        "/CN=localhost",
    ]);
    std::fs::write(
        root.path().join("extensions.cnf"),
        "subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n",
    )
    .unwrap();
    run(&[
        "x509",
        "-req",
        "-in",
        "server.csr",
        "-CA",
        "ca.pem",
        "-CAkey",
        "ca.key",
        "-CAcreateserial",
        "-out",
        "server.pem",
        "-days",
        "1",
        "-extfile",
        "extensions.cnf",
    ]);
    let mut server = Command::new("python3")
        .arg("-u")
        .arg("-c")
        .arg(include_str!("tls_fixture.py"))
        .current_dir(root.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut port = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut port)
        .unwrap();
    let mut results = Vec::new();
    for trusted in [true, false] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "tls_tests::provider_uses_system_trust_for_https_catalogs",
                "--nocapture",
            ])
            .env(
                "ANTEX_TLS_FIXTURE",
                format!("https://localhost:{}", port.trim()),
            )
            .env("NO_PROXY", "localhost,127.0.0.1");
        if trusted {
            command
                .env("SSL_CERT_FILE", root.path().join("ca.pem"))
                .env("SSL_CERT_DIR", root.path());
        } else {
            command
                .env_remove("SSL_CERT_FILE")
                .env_remove("SSL_CERT_DIR")
                .env("ANTEX_TLS_EXPECT_REJECT", "1");
        }
        results.push(command.output().unwrap());
    }
    let _ = server.kill();
    let _ = server.wait();
    for output in results {
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
