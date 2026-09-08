use super::*;
use pretty_assertions::assert_eq;
use std::time::Duration;

#[cfg(unix)]
#[tokio::test]
async fn workflow_controls_stay_responsive_and_stop_cancels_shell_descendants() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    crate::first_party_extensions::install(home.path(), "workflows").unwrap();
    std::fs::create_dir(home.path().join("workflows")).unwrap();
    std::fs::write(home.path().join("workflows/check.py"), "WORKFLOW={'id':'check'}\ndef run(ctx):\n if ctx.state.get('started'): return 'resumed'\n ctx.checkpoint({'started':True})\n return ctx.shell(['/bin/sh','-c','printf ready > ready; sleep 1; printf escaped > escaped'])\n").unwrap();
    let provider = OpenAiProvider::new(home.path()).unwrap();
    let mut session = InteractiveSession::new(
        home.path().into(),
        workspace.path().into(),
        Config::default(),
        std::env::var_os("ANTEX_BWRAP").map(Into::into),
        provider,
    )
    .unwrap();
    session.command("/workflow check").await.unwrap();
    session.background_notice().await.unwrap();
    let CommandEffect::Notice(paused) = session.command("/workflow pause").await.unwrap() else {
        panic!("expected pause notice");
    };
    insta::assert_snapshot!(paused);
    tokio::time::timeout(Duration::from_millis(100), session.command("/status"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(workspace.path().join("ready").exists(), false);
    assert!(session.command("/cd ..").await.is_err());
    session.command("/workflow resume").await.unwrap();
    session.background_notice().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !workspace.path().join("ready").exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let CommandEffect::Notice(stopped) = session.command("/workflow stop").await.unwrap() else {
        panic!("expected stop notice");
    };
    insta::assert_snapshot!(stopped);
    assert_eq!(
        session.conversation.extension_states().unwrap()["workflows"]["phase"],
        "stopped"
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(workspace.path().join("escaped").exists(), false);
    assert_eq!(
        super::tests::complete_workflow(&mut session, "/workflow resume").await,
        "\"resumed\""
    );
}
