use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use serde_json::json;

const MAX_ENTRIES: usize = 10_000;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Parser)]
#[command(bin_name = "antex migrate")]
struct MigrationCli {
    #[command(subcommand)]
    source: Source,
}

#[derive(Subcommand)]
enum Source {
    Codex {
        #[arg(long)]
        dry_run: bool,
    },
}

enum Content {
    File(PathBuf),
    Config(Vec<u8>),
}

struct Import {
    relative: PathBuf,
    content: Content,
}

pub(super) fn run() -> anyhow::Result<()> {
    let Source::Codex { dry_run } = MigrationCli::parse_from(std::env::args_os().skip(1)).source;
    let source = std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
        .context("cannot locate the Codex import source")?
        .canonicalize()
        .context("Codex import source must exist")?;
    let destination = super::antex_entry::home()?;
    anyhow::ensure!(
        !destination.starts_with(&source) && !source.starts_with(&destination),
        "Codex import source and Antex destination must not overlap"
    );
    let mut directories = vec![source.clone()];
    let mut imports = Vec::new();
    let mut report = Vec::new();
    let mut count = 0;
    while let Some(directory) = directories.pop() {
        let mut entries = fs::read_dir(&directory)?
            .take(MAX_ENTRIES + 1)
            .collect::<Result<Vec<_>, _>>()?;
        count += entries.len();
        anyhow::ensure!(count <= MAX_ENTRIES, "import exceeds {MAX_ENTRIES} entries");
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(&source)?.to_path_buf();
            let kind = entry.file_type()?;
            let supported = relative == Path::new("config.toml")
                || ["skills", "sessions", "prompt-stashes"]
                    .iter()
                    .any(|root| relative.starts_with(root));
            if !supported || kind.is_symlink() || !(kind.is_dir() || kind.is_file()) {
                report.push(json!({"action": "skip", "path": relative, "reason": "unsupported entry or symlink"}));
                continue;
            }
            validate_destination(&destination, &relative)?;
            if kind.is_dir() {
                directories.push(path);
                continue;
            }
            if destination.join(&relative).exists() {
                report.push(
                    json!({"action": "skip", "path": relative, "reason": "destination exists"}),
                );
                continue;
            }
            let content = if relative == Path::new("config.toml") {
                let mut input = String::new();
                File::open(&path)?
                    .take(MAX_CONFIG_BYTES + 1)
                    .read_to_string(&mut input)?;
                anyhow::ensure!(
                    input.len() as u64 <= MAX_CONFIG_BYTES,
                    "config exceeds 1 MiB"
                );
                let mut config: toml::Table =
                    toml::from_str(&input).context("invalid source config")?;
                let mut skipped = Vec::new();
                config.retain(|key, _| {
                    let keep = matches!(
                        key,
                        "model"
                            | "model_reasoning_effort"
                            | "model_reasoning_summary"
                            | "model_verbosity"
                            | "model_context_window"
                            | "model_auto_compact_token_limit"
                            | "approval_policy"
                            | "sandbox_mode"
                            | "mcp_servers"
                            | "skills"
                            | "tui"
                    );
                    if !keep {
                        skipped.push(key.to_owned());
                    }
                    keep
                });
                for (section, allowed) in [
                    ("tui", &["theme"][..]),
                    (
                        "skills",
                        &["config", "include_instructions", "max_context_tokens"][..],
                    ),
                ] {
                    if let Some(table) = config.get_mut(section).and_then(toml::Value::as_table_mut)
                    {
                        table.retain(|key, _| {
                            let keep = allowed.contains(&key);
                            if !keep {
                                skipped.push(format!("{section}.{key}"));
                            }
                            keep
                        });
                    }
                }
                if let Some(servers) = config
                    .get_mut("mcp_servers")
                    .and_then(toml::Value::as_table_mut)
                {
                    for (name, server) in servers {
                        if let Some(fields) = server.as_table_mut() {
                            fields.retain(|key, _| {
                                let keep = matches!(
                                    key,
                                    "command"
                                        | "args"
                                        | "env"
                                        | "env_vars"
                                        | "cwd"
                                        | "url"
                                        | "bearer_token_env_var"
                                        | "http_headers"
                                        | "env_http_headers"
                                        | "enabled"
                                        | "required"
                                        | "startup_timeout_sec"
                                        | "startup_timeout_ms"
                                        | "tool_timeout_sec"
                                        | "enabled_tools"
                                        | "disabled_tools"
                                );
                                if !keep {
                                    skipped.push(format!("mcp_servers.{name}.{key}"));
                                }
                                keep
                            });
                        }
                    }
                }
                if let Some(rules) = config
                    .get_mut("skills")
                    .and_then(|skills| skills.get_mut("config"))
                    .and_then(toml::Value::as_array_mut)
                {
                    for (index, rule) in rules.iter_mut().enumerate() {
                        if let Some(fields) = rule.as_table_mut() {
                            fields.retain(|key, _| {
                                let keep = matches!(key, "path" | "name" | "enabled");
                                if !keep {
                                    skipped.push(format!("skills.config.{index}.{key}"));
                                }
                                keep
                            });
                        }
                        if let Some(value) = rule.get_mut("path")
                            && let Some(path) = value.as_str()
                        {
                            let path = PathBuf::from(path);
                            let resolved = path.canonicalize().unwrap_or(path);
                            let replacement = match resolved.strip_prefix(&source) {
                                Ok(relative) => destination.join(relative),
                                Err(_) => resolved,
                            };
                            *value = toml::Value::String(
                                replacement
                                    .to_str()
                                    .context("skill path is not UTF-8")?
                                    .to_owned(),
                            );
                        }
                    }
                }
                for key in skipped {
                    report.push(json!({"action": "skip-config", "key": key}));
                }
                let output = toml::to_string(&config)?;
                let _: codex_config::config_toml::ConfigToml = toml::from_str(&output)
                    .context("supported imported configuration is invalid")?;
                Content::Config(output.into_bytes())
            } else {
                anyhow::ensure!(
                    entry.metadata()?.len() <= MAX_FILE_BYTES,
                    "import file exceeds 64 MiB: {}",
                    relative.display()
                );
                Content::File(path)
            };
            report.push(json!({"action": "copy", "path": relative}));
            imports.push(Import { relative, content });
        }
    }
    report.sort_by_key(serde_json::Value::to_string);
    let output = serde_json::to_string(&json!({"dryRun": dry_run, "actions": report}))?;
    if dry_run {
        println!("{output}");
        return Ok(());
    }
    fs::create_dir_all(&destination)?;
    for import in imports {
        validate_destination(&destination, &import.relative)?;
        let target = destination.join(&import.relative);
        let parent = target.parent().context("import target has no parent")?;
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        match import.content {
            Content::File(source) => {
                let copied = std::io::copy(
                    &mut File::open(source)?.take(MAX_FILE_BYTES + 1),
                    &mut temporary,
                )?;
                anyhow::ensure!(copied <= MAX_FILE_BYTES, "import file grew beyond 64 MiB");
            }
            Content::Config(bytes) => temporary.write_all(&bytes)?,
        }
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&target).with_context(|| {
            format!(
                "cannot import {} without overwriting",
                import.relative.display()
            )
        })?;
    }
    println!("{output}");
    Ok(())
}

fn validate_destination(root: &Path, relative: &Path) -> anyhow::Result<()> {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => anyhow::ensure!(
                !metadata.is_symlink(),
                "import destination contains a symlink: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
