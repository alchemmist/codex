use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use antex_extension_host::ExtensionLaunch;
use antex_extension_host::ExtensionLauncher;
use antex_extension_host::ExtensionRegistry;
use antex_extension_protocol::Event;
use antex_runtime::LocalRuntime;
use antex_runtime::PermissionProfile;
use futures::future::BoxFuture;
use serde_json::json;

struct DirectLauncher;

impl ExtensionLauncher for DirectLauncher {
    fn command<'a>(
        &'a self,
        launch: ExtensionLaunch<'a>,
    ) -> BoxFuture<'a, std::io::Result<tokio::process::Command>> {
        Box::pin(async move {
            let mut command = tokio::process::Command::new(launch.program);
            command.args(launch.arguments).current_dir(launch.cwd);
            Ok(command)
        })
    }
}

#[tokio::test]
async fn registry_delivers_only_subscribed_events_and_isolates_failures() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let directory = home.path().join("extensions/fixture");
    fs::create_dir_all(&directory).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance.py");
    let program = directory.join("fixture.py");
    let mut bytes = b"#!/usr/bin/env python3\n".to_vec();
    bytes.extend(fs::read(source).unwrap());
    fs::write(&program, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(
        directory.join("extension.json"),
        json!({"name":"fixture","program":"fixture.py"}).to_string(),
    )
    .unwrap();
    let runtime = Arc::new(LocalRuntime::new(workspace.path(), PermissionProfile::Full).unwrap());
    let loaded = ExtensionRegistry::load(
        home.path(),
        workspace.path(),
        /*project_trusted*/ false,
        runtime,
        "0.0.0",
        "session-1",
        Arc::new(DirectLauncher),
    )
    .await
    .unwrap();
    assert!(loaded.failures.is_empty());
    assert!(
        loaded
            .registry
            .notify(Event {
                name: "notSubscribed".into(),
                data: json!({"fail":true}),
            })
            .await
            .is_empty()
    );
    assert_eq!(
        loaded
            .registry
            .notify(Event {
                name: "turnComplete".into(),
                data: json!({"fail":true}),
            })
            .await
            .len(),
        1
    );
    assert!(
        loaded
            .registry
            .notify(Event {
                name: "turnComplete".into(),
                data: json!({}),
            })
            .await
            .is_empty()
    );
}
