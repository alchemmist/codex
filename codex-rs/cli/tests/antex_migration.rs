use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::TempDir;

fn import(source: &Path, destination: &Path) -> Result<Command> {
    let mut command = Command::new(cargo_bin("antex")?);
    command
        .env("CODEX_HOME", source)
        .env("ANTEX_HOME", destination)
        .args(["migrate", "codex"]);
    Ok(command)
}

#[test]
fn dry_run_is_deterministic_and_does_not_copy_or_expose_credentials() -> Result<()> {
    let source = TempDir::new()?;
    let destination = TempDir::new()?;
    fs::write(
        source.path().join("config.toml"),
        "model = 'example'\nunknown = 'private-value'\n[tui]\nunknown = true\n",
    )?;
    fs::write(source.path().join("auth.json"), "private-credential")?;
    let first = import(source.path(), destination.path())?
        .arg("--dry-run")
        .output()?;
    let second = import(source.path(), destination.path())?
        .arg("--dry-run")
        .output()?;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(second.status.success());
    assert_eq!(&first.stdout, &second.stdout);
    let report = String::from_utf8(first.stdout)?;
    assert!(report.contains("tui.unknown"));
    assert!(report.contains("skip-config"));
    assert!(!report.contains("private-value"));
    assert!(!report.contains("private-credential"));
    assert_eq!(fs::read_dir(destination.path())?.count(), 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn dry_run_does_not_create_the_default_antex_home() -> Result<()> {
    let source = TempDir::new()?;
    let user_home = TempDir::new()?;
    fs::write(source.path().join("config.toml"), "model = 'example'\n")?;
    let output = import(source.path(), user_home.path())?
        .env_remove("ANTEX_HOME")
        .env("HOME", user_home.path())
        .arg("--dry-run")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!user_home.path().join(".antex").exists());
    Ok(())
}

#[test]
fn import_copies_supported_data_and_rerun_preserves_destination_changes() -> Result<()> {
    let source = TempDir::new()?;
    let destination = TempDir::new()?;
    let config = "model = 'example'\nunknown = true\n[mcp_servers.fixture]\ncommand = 'echo'\nargs = ['fixture']\nunknown = true\n";
    let skill_path = source.path().join("skills/example/SKILL.md");
    let config = format!("{config}\n[[skills.config]]\npath = {skill_path:?}\nenabled = true\n");
    fs::write(source.path().join("config.toml"), &config)?;
    fs::write(source.path().join("auth.json"), "private-credential")?;
    fs::create_dir_all(source.path().join("skills/example"))?;
    fs::write(&skill_path, "fixture skill")?;
    fs::create_dir(source.path().join("prompt-stashes"))?;
    fs::write(source.path().join("prompt-stashes/fixture.json"), "{}")?;
    fs::create_dir(source.path().join("sessions"))?;
    fs::write(
        source.path().join("sessions/fixture.jsonl"),
        "{\"fixture\":true}\n",
    )?;
    let output = import(source.path(), destination.path())?.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: toml::Value =
        toml::from_str(&fs::read_to_string(destination.path().join("config.toml"))?)?;
    let imported_skill = destination
        .path()
        .canonicalize()?
        .join("skills/example/SKILL.md");
    let expected: toml::Value = toml::from_str(&format!(
        "model = 'example'\n[mcp_servers.fixture]\ncommand = 'echo'\nargs = ['fixture']\n[[skills.config]]\npath = {imported_skill:?}\nenabled = true\n"
    ))?;
    assert_eq!(parsed, expected);
    assert!(!destination.path().join("auth.json").exists());
    assert_eq!(
        fs::read_to_string(source.path().join("config.toml"))?,
        config
    );
    assert_eq!(
        fs::read(destination.path().join("sessions/fixture.jsonl"))?,
        fs::read(source.path().join("sessions/fixture.jsonl"))?
    );
    assert_eq!(fs::read(imported_skill)?, fs::read(skill_path)?);
    assert_eq!(
        fs::read_to_string(destination.path().join("prompt-stashes/fixture.json"))?,
        "{}"
    );
    fs::write(
        destination.path().join("config.toml"),
        "model = 'modified'\n",
    )?;
    let repeated = import(source.path(), destination.path())?.output()?;
    assert!(repeated.status.success());
    assert_eq!(
        fs::read_to_string(destination.path().join("config.toml"))?,
        "model = 'modified'\n"
    );
    let report: Value = serde_json::from_slice(&repeated.stdout)?;
    assert!(
        report["actions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|action| action["action"] == "skip")
    );
    Ok(())
}

#[test]
fn overlapping_source_and_destination_are_rejected() -> Result<()> {
    let directory = TempDir::new()?;
    let output = import(directory.path(), directory.path())?.output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("must not overlap"));
    assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn destination_symlink_cannot_redirect_session_writes_to_source() -> Result<()> {
    let source = TempDir::new()?;
    let destination = TempDir::new()?;
    fs::create_dir(source.path().join("sessions"))?;
    fs::write(source.path().join("sessions/fixture.jsonl"), "original\n")?;
    std::os::unix::fs::symlink(
        source.path().join("sessions"),
        destination.path().join("sessions"),
    )?;
    let output = import(source.path(), destination.path())?.output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("contains a symlink"));
    assert_eq!(
        fs::read_to_string(source.path().join("sessions/fixture.jsonl"))?,
        "original\n"
    );
    Ok(())
}
