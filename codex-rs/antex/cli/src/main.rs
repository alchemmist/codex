use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use antex_core::Agent;
use antex_core::AgentCommand;
use antex_core::AgentEvent;
use antex_core::FinishReason;
use antex_core::InteractionAnswer;
use antex_core::ModelProvider;
use antex_core::TurnInput;
use antex_provider_openai::OpenAiProvider;
use antex_runtime::Compaction;
use antex_runtime::Config;
use antex_runtime::Conversation;
use antex_runtime::ImageAttachment;
use antex_runtime::LocalRuntime;
use antex_runtime::PermissionProfile;
use antex_runtime::ProjectContext;
use antex_runtime::SessionStore;
use clap::Parser;
use clap::Subcommand;
use tokio_util::sync::CancellationToken;

mod interactive;

#[derive(Parser)]
#[command(name="antex",version=env!("ANTEX_BUILD_VERSION"),about="Antex terminal coding agent")]
struct Args {
    #[arg(long, env = "ANTEX_HOME", global = true)]
    home: Option<PathBuf>,
    #[arg(short = 'C', long = "cd", global = true)]
    cwd: Option<PathBuf>,
    #[arg(long,value_parser=["read-only","workspace","full"],global=true)]
    permissions: Option<String>,
    #[arg(long, env = "ANTEX_BWRAP", global = true)]
    bubblewrap: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    Tui,
    Login {
        #[arg(long)]
        device_code: bool,
        #[arg(long, default_value = "default")]
        account: String,
    },
    Accounts,
    UseAccount {
        name: String,
    },
    Logout {
        name: String,
    },
    Models,
    Sessions,
    Compact {
        session: String,
        #[arg(long)]
        model: Option<String>,
    },
    Exec {
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        reasoning: Option<String>,
        #[arg(long = "image", value_name = "PATH")]
        images: Vec<PathBuf>,
        #[arg(long)]
        resume: Option<String>,
        #[arg(long, requires = "resume")]
        fork_at: Option<String>,
        prompt: String,
    },
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run(Args::parse()).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("antex: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let Some(action) = args.command else {
        return Err(
            "the direct interactive frontend is still being migrated; use `antex exec`".into(),
        );
    };
    let user_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is unavailable")?
        .canonicalize()?;
    let home = match args.home {
        Some(home) => home.canonicalize()?,
        None => {
            let home = user_home.join(".antex");
            if home.exists() {
                home.canonicalize()?
            } else {
                std::fs::create_dir(&home)?;
                home
            }
        }
    };
    let legacy = user_home.join(".codex");
    if home.starts_with(legacy.canonicalize().unwrap_or(legacy)) {
        return Err("Antex home must not be inside the legacy .codex directory".into());
    }
    let provider = OpenAiProvider::new(&home)?;
    let loaded = Config::load(&home)?;
    for warning in loaded.warnings {
        eprintln!("antex: {warning}");
    }
    let config = loaded.config;
    match action {
        Action::Tui => {
            let workspace = args
                .cwd
                .unwrap_or(std::env::current_dir()?)
                .canonicalize()?;
            let mut config = config;
            if let Some(profile) = args.permissions {
                config.permissions = match profile.as_str() {
                    "read-only" => PermissionProfile::ReadOnly,
                    "workspace" => PermissionProfile::Workspace,
                    "full" => PermissionProfile::Full,
                    _ => return Err("invalid permission profile".into()),
                };
            }
            let settings = antex_tui::Settings::from_config(&config.tui, home.clone())
                .map_err(std::io::Error::other)?;
            let mut session = interactive::InteractiveSession::new(
                home,
                workspace,
                config,
                args.bubblewrap,
                provider,
            )
            .map_err(std::io::Error::other)?;
            antex_tui::run(&mut session, settings).await?;
        }
        Action::Login {
            device_code,
            account,
        } => {
            let cancellation = CancellationToken::new();
            let signal = cancellation.clone();
            let task = tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                signal.cancel();
            });
            let result = if device_code {
                let login = provider.begin_device_login().await?;
                println!(
                    "Open {} and enter {}\nContinue only if you started this login in Antex.",
                    login.verification_url, login.user_code
                );
                provider
                    .finish_device_login(login, account, cancellation)
                    .await
            } else {
                let login = provider.begin_browser_login().await?;
                println!(
                    "Open this URL to sign in to Antex:\n{}",
                    login.authorization_url
                );
                provider
                    .finish_browser_login(login, account, cancellation)
                    .await
            };
            task.abort();
            result?;
            println!("Signed in to Antex.");
        }
        Action::Accounts => {
            for account in provider.accounts().await? {
                println!("{account}");
            }
        }
        Action::UseAccount { name } => provider.select_account(&name).await?,
        Action::Logout { name } => provider.logout(&name).await?,
        Action::Models => {
            for model in provider.models().await? {
                println!("{}\t{}", model.id, model.display_name);
            }
        }
        Action::Sessions => {
            let workspace = args
                .cwd
                .unwrap_or(std::env::current_dir()?)
                .canonicalize()?;
            for id in SessionStore::new(&home, &workspace)?.list()? {
                println!("{id}");
            }
        }
        Action::Compact { session, model } => {
            let workspace = args
                .cwd
                .unwrap_or(std::env::current_dir()?)
                .canonicalize()?;
            let store = SessionStore::new(&home, &workspace)?;
            let mut session = store.open(session.parse()?)?;
            session.recover_pending_tools()?;
            let entries = session.active_path()?;
            let ids = entries
                .iter()
                .map(|entry| entry.record_id)
                .collect::<Vec<_>>();
            let history = entries
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>();
            if history.len() <= 2 {
                println!("Session is already small; no compaction needed.");
                return Ok(());
            }
            let model = match model.or(config.model) {
                Some(model) => model,
                None => {
                    provider
                        .models()
                        .await?
                        .into_iter()
                        .next()
                        .ok_or("no models available")?
                        .id
                }
            };
            let compaction = Compaction::new(
                Arc::new(provider),
                ProjectContext::load(&home, &workspace)?,
                model,
                config.context_token_limit,
            );
            let checkpoint = tokio::select! {
                result=compaction.compact(&history)=>result?,
                _=tokio::signal::ctrl_c()=>return Err("compaction cancelled".into()),
            };
            let retained = checkpoint
                .retained
                .iter()
                .map(|index| ids[*index])
                .collect::<Vec<_>>();
            session.checkpoint(checkpoint.summary, &retained, ids[checkpoint.tail_start])?;
            session.finish_turn()?;
            println!("Compacted session {}", session.id());
        }
        Action::Exec {
            model,
            reasoning,
            images,
            resume,
            fork_at,
            prompt,
        } => {
            let workspace = args
                .cwd
                .unwrap_or(std::env::current_dir()?)
                .canonicalize()?;
            if images.len() > 4 {
                return Err("at most four images may be attached to one input".into());
            }
            let mut content = Vec::new();
            if !prompt.is_empty() {
                content.push(antex_core::Content::Text(prompt));
            }
            for path in images {
                content.push(ImageAttachment::load(&workspace.join(path))?.content);
            }
            if content.is_empty() {
                return Err("input must contain text or an image".into());
            }
            let profile = match args.permissions.as_deref() {
                Some("read-only") => PermissionProfile::ReadOnly,
                Some("workspace") => PermissionProfile::Workspace,
                Some("full") => PermissionProfile::Full,
                None => config.permissions,
                Some(_) => return Err("invalid permission profile".into()),
            };
            let context = ProjectContext::load(&home, &workspace)?;
            for warning in &context.warnings {
                eprintln!("antex: {warning}");
            }
            let mut runtime = LocalRuntime::new(&workspace, profile)?
                .with_read_roots(&context.read_roots)?
                .with_shell_timeout(std::time::Duration::from_secs(config.shell_timeout_seconds));
            if let Some(program) = args.bubblewrap {
                runtime = runtime.with_bubblewrap(program);
            }
            let model = match model.or(config.model) {
                Some(model) => model,
                None => {
                    provider
                        .models()
                        .await?
                        .into_iter()
                        .next()
                        .ok_or("no models available")?
                        .id
                }
            };
            let store = SessionStore::new(&home, &workspace)?;
            let mut session = match resume {
                Some(id) => store.open(id.parse()?)?,
                None => store.create()?,
            };
            if let Some(parent) = fork_at {
                session.branch(parent.parse()?)?;
            }
            let mut conversation = Conversation::new(session)?;
            let history = conversation.messages();
            eprintln!("Session: {}", conversation.id());
            let provider = Arc::new(provider);
            let compaction = Compaction::new(
                provider.clone(),
                context,
                model.clone(),
                config.context_token_limit,
            );
            let mut agent =
                Agent::new(provider, Arc::new(runtime)).with_context_hook(Arc::new(compaction));
            let mut run = agent.start(TurnInput {
                model,
                reasoning: reasoning.or(config.model_reasoning_effort),
                history,
                input: antex_core::UserInput {
                    content,
                    tool_scope: antex_core::ToolScope::Default,
                },
            });
            let commands = run.commands.clone();
            let signal = tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                let _ = commands.send(AgentCommand::Interrupt).await;
            });
            while let Some(event) = run.events.recv().await {
                conversation.record(&event)?;
                match event {
                    AgentEvent::ContextCheckpoint(_) => {
                        eprintln!("antex: context compacted");
                    }
                    AgentEvent::TextDelta(text) => {
                        print!("{text}");
                        std::io::stdout().flush()?;
                    }
                    AgentEvent::Interaction { request, .. } => {
                        eprintln!("antex exec: interactive request denied in noninteractive mode");
                        let _ = request.answer(InteractionAnswer::Deny);
                    }
                    AgentEvent::Error(error) => eprintln!("\nantex: {error}"),
                    AgentEvent::Finished { reason, .. } => {
                        signal.abort();
                        println!();
                        if reason != FinishReason::Completed {
                            return Err(format!("run ended: {reason:?}").into());
                        }
                    }
                    AgentEvent::MessageCommitted(_)
                    | AgentEvent::ReasoningDelta(_)
                    | AgentEvent::Quota(_)
                    | AgentEvent::ToolStarted(_)
                    | AgentEvent::ToolProgress { .. }
                    | AgentEvent::Usage(_)
                    | AgentEvent::TurnCompleted => {}
                }
            }
        }
    }
    Ok(())
}
