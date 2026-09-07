use std::fs;

use pretty_assertions::assert_eq;

use super::MigrationPlan;

#[test]
fn codex_migration_is_deterministic_and_never_changes_the_source() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let original = "model = 'gpt-test'\nsandbox_mode = 'workspace-write'\nsecret = 'do-not-echo'\n[mcp_servers.docs]\ncommand = 'docs-server'\nargs = ['--stdio']\n[mcp_servers.remote]\nurl = 'https://example.test/mcp'\n";
    fs::write(source.path().join("config.toml"), original).unwrap();
    fs::create_dir_all(source.path().join("skills/review/scripts")).unwrap();
    fs::write(source.path().join("skills/review/SKILL.md"), "# Review\n").unwrap();
    fs::write(
        source.path().join("skills/review/scripts/check.py"),
        "print('ok')\n",
    )
    .unwrap();
    let first = MigrationPlan::codex(source.path(), destination.path()).unwrap();
    let second = MigrationPlan::codex(source.path(), destination.path()).unwrap();
    assert_eq!(first.describe(), second.describe());
    assert!(!first.describe().join("\n").contains("do-not-echo"));
    assert!(
        first
            .describe()
            .iter()
            .any(|line| line == "skip MCP remote: streamable HTTP/OAuth is not implemented")
    );
    first.apply().unwrap();
    assert_eq!(
        fs::read_to_string(source.path().join("config.toml")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("skills/review/SKILL.md")).unwrap(),
        "# Review\n"
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("skills/review/scripts/check.py")).unwrap(),
        "print('ok')\n"
    );
    assert_eq!(
        fs::read_to_string(destination.path().join("config.toml")).unwrap(),
        "model = \"gpt-test\"\npermissions = \"workspace\"\n"
    );
    let definition: serde_json::Value = serde_json::from_slice(
        &fs::read(
            destination
                .path()
                .join("extensions/mcp-docs/extension.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        definition,
        serde_json::json!({
            "name":"docs",
            "program":"antex_ext_mcp.py",
            "arguments":["--name","docs","--","docs-server","--stdio"],
            "capabilities":["network","shell"]
        })
    );
}
