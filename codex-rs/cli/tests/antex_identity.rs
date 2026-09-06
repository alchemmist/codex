use std::fs;

use anyhow::Result;
use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

#[test]
fn version_does_not_initialize_runtime_state() -> Result<()> {
    let directory = TempDir::new()?;
    let target = directory.path().join("not-created");
    let output = std::process::Command::new(cargo_bin("antex")?)
        .env("ANTEX_HOME", &target)
        .arg("--version")
        .output()?;
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.starts_with("antex 0.0.0+"));
    assert!(!target.exists());
    Ok(())
}

#[test]
fn mcp_config_writes_use_antex_home_and_ignore_legacy_home_override() -> Result<()> {
    let antex_home = TempDir::new()?;
    let legacy_home = TempDir::new()?;
    let sentinel = "model = 'legacy-sentinel'\n";
    fs::write(legacy_home.path().join("config.toml"), sentinel)?;
    let output = std::process::Command::new(cargo_bin("antex")?)
        .env("ANTEX_HOME", antex_home.path())
        .env("CODEX_HOME", legacy_home.path())
        .env("CODEX_SQLITE_HOME", legacy_home.path())
        .args(["mcp", "add", "fixture", "--", "echo", "fixture"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config: toml::Value = fs::read_to_string(antex_home.path().join("config.toml"))?.parse()?;
    assert_eq!(
        config["mcp_servers"]["fixture"]["command"].as_str(),
        Some("echo")
    );
    assert_eq!(
        fs::read_to_string(legacy_home.path().join("config.toml"))?,
        sentinel
    );
    assert_eq!(fs::read_dir(legacy_home.path())?.count(), 1);
    Ok(())
}

#[cfg(unix)]
#[test]
fn default_home_is_separate_from_codex() -> Result<()> {
    let user_home = TempDir::new()?;
    let output = std::process::Command::new(cargo_bin("antex")?)
        .env("HOME", user_home.path())
        .env_remove("ANTEX_HOME")
        .args(["mcp", "add", "fixture", "--", "echo", "fixture"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(user_home.path().join(".antex/config.toml").is_file());
    assert!(!user_home.path().join(".codex").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn home_alias_into_codex_is_rejected_before_writing() -> Result<()> {
    let user_home = TempDir::new()?;
    let codex_home = user_home.path().join(".codex");
    fs::create_dir(&codex_home)?;
    let alias = user_home.path().join("alias");
    std::os::unix::fs::symlink(&codex_home, &alias)?;
    let output = std::process::Command::new(cargo_bin("antex")?)
        .env("HOME", user_home.path())
        .env("ANTEX_HOME", &alias)
        .args(["mcp", "list"])
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("legacy .codex"));
    assert_eq!(fs::read_dir(codex_home)?.count(), 0);
    Ok(())
}
