use std::io;
use std::io::IsTerminal;
use std::sync::Arc;

use antex_core::AgentCommand;
use antex_core::AgentEvent;
use antex_core::AgentRun;
use crossterm::event::Event;
use crossterm::event::EventStream;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;

use crate::CommandEffect;
use crate::Session;
use crate::composer::Composer;
use crate::composer::SubmitMode;
use crate::frontend_layout::draw;
use crate::insert_history::insert_history_lines;
use crate::keymap::RuntimeKeymap;
use crate::terminal_guard::TerminalGuard;
use crate::transcript::safe_text;
use crate::transcript::write_message;
use crate::tui::Tui;

pub async fn run(session: &mut impl Session) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "interactive mode requires a terminal; use antex exec for pipes",
        ));
    }
    let mut guard = TerminalGuard::enter()?;
    let probe = crate::terminal_probe::startup(
        crate::terminal_probe::DEFAULT_TIMEOUT,
        crate::terminal_probe::StartupKeyboardEnhancementProbe::Query,
    )
    .ok();
    crate::terminal_palette::set_default_colors_from_startup_probe(
        probe.and_then(|probe| probe.default_colors),
    );
    if probe.is_some_and(|probe| probe.keyboard_enhancement_supported == Some(true)) {
        guard.enable_enhanced_keys()?;
    }
    let cursor = probe
        .and_then(|probe| probe.cursor_position)
        .unwrap_or_default();
    let mut tui = Tui::with_cursor(CrosstermBackend::new(io::stdout()), cursor)?;
    run_terminal(&mut tui, session, EventStream::new()).await
}

async fn run_terminal<B, E>(
    tui: &mut Tui<B>,
    session: &mut impl Session,
    mut input: E,
) -> io::Result<()>
where
    B: ratatui::backend::Backend<Error = io::Error> + io::Write,
    E: futures::Stream<Item = io::Result<Event>> + Unpin,
{
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    let mut run: Option<AgentRun> = None;
    let mut status = String::new();
    let mut live = String::new();
    let mut pending = session.pending_commands().map_err(io::Error::other)?;
    let mut prompt = None;
    let mut closing = false;
    let frames = crate::tui::FrameRequester::new();
    let mut startup = Some(crate::startup::Startup::new(
        crate::StartupMascotSkin::default(),
        frames.clone(),
    ));
    draw(
        tui,
        &mut composer,
        session,
        &status,
        &live,
        &mut prompt,
        &startup,
    )?;
    let history = session.history();
    if !history.is_empty()
        && let Some(panel) = startup.take()
    {
        let width = tui.terminal.last_known_screen_size.width;
        insert_history_lines(&mut tui.terminal, panel.final_lines(width))?;
    }
    for message in history {
        write_message(&mut tui.terminal, &message, &session.view().directory)?;
    }
    let frame_budget = std::time::Duration::from_millis(33);
    let mut last_draw = std::time::Instant::now();
    let mut force_draw = true;
    loop {
        if closing && run.is_none() {
            break;
        }
        let elapsed = last_draw.elapsed();
        if force_draw || elapsed >= frame_budget {
            draw(
                tui,
                &mut composer,
                session,
                &status,
                &live,
                &mut prompt,
                &startup,
            )?;
            last_draw = std::time::Instant::now();
            force_draw = false;
        } else {
            frames.schedule_frame_in(frame_budget - elapsed);
        }
        tokio::select! {
            _ = frames.next_frame() => {},
            event = next_agent_event(&mut run) => {
                let Some(event) = event else { run = None; continue; };
                if let Err(error) = session.record(&event) {
                    if let Some(active) = &run { let _ = active.commands.try_send(AgentCommand::Interrupt); }
                    return Err(io::Error::other(error));
                }
                match event {
                    AgentEvent::TextDelta(text) => {
                        live.push_str(&safe_text(&text));
                        if live.len() > 16_000 {
                            let mut start = live.len() - 16_000;
                            while !live.is_char_boundary(start) { start += 1; }
                            live.drain(..start);
                        }
                    }
                    AgentEvent::MessageCommitted(message) => {
                        if let Some(panel) = startup.take() {
                            draw(tui, &mut composer, session, &status, &live, &mut prompt, &startup)?;
                            let width = tui.terminal.last_known_screen_size.width;
                            insert_history_lines(&mut tui.terminal, panel.final_lines(width))?;
                        }
                        write_message(&mut tui.terminal, &message, &session.view().directory)?;
                        live.clear();
                    }
                    AgentEvent::ToolStarted(call) => {
                        status = format!("Running {}", safe_text(&call.name));
                        let preview = crate::transcript::tool_preview(&call, usize::from(tui.terminal.last_known_screen_size.width), &session.view().directory);
                        if !preview.is_empty() { insert_history_lines(&mut tui.terminal, preview)?; }
                    },
                    AgentEvent::ToolProgress { text, .. } => status = safe_text(&text),
                    AgentEvent::Interaction { request, .. } => {
                        let pane = crate::prompt::Prompt::new(request);
                        insert_history_lines(&mut tui.terminal, pane.description())?;
                        prompt = Some(pane);
                        status = "Waiting for your answer".into();
                    }
                    AgentEvent::Error(error) => status = safe_text(&error.to_string()),
                    AgentEvent::Finished { reason, pending: remaining } => {
                        pending.extend(remaining);
                        status = format!("{reason:?}; {} unsent inputs retained", pending.len());
                        run = None;
                        prompt = None;
                        live.clear();
                    }
                    AgentEvent::ContextCheckpoint(_) => status = "Context compacted".into(),
                    AgentEvent::Usage(usage) => status = format!("{} input · {} output tokens", usage.input_tokens, usage.output_tokens),
                    AgentEvent::Quota(_) | AgentEvent::ReasoningDelta(_) | AgentEvent::TurnCompleted => {}
                }
            }
            event = next_input_event(&mut input, closing) => {
                let Some(event) = event else {
                    closing = true;
                    if let Some(active) = &run { let _ = active.commands.try_send(AgentCommand::Interrupt); }
                    continue;
                };
                force_draw = true;
                match event? {
                    Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Release => continue,
                    Event::Key(key) if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') => {
                        if let Some(active) = &run { let _ = active.commands.try_send(AgentCommand::Interrupt); }
                        else { break; }
                    }
                    Event::Key(key) => {
                        if let Some(pane) = &mut prompt {
                            match pane.key(key) {
                                Ok(true) => prompt = None,
                                Ok(false) => {},
                                Err(error) => status = safe_text(&error),
                            }
                            continue;
                        }
                        match composer.key(key) {
                            Err(error) => status = error.into(),
                            Ok(None) => {},
                            Ok(Some(mode)) => {
                                let draft = composer.draft();
                                let user = draft.input();
                                let text = user.content.iter().filter_map(|content| match content { antex_core::Content::Text(text) => Some(text.as_str()), _ => None }).collect::<String>();
                                if user.content.is_empty() { continue; }
                                if let Some(active) = &run {
                                    if !pending.is_empty() {
                                        status = "Recover the remaining unsent inputs before queueing more.".into();
                                        continue;
                                    }
                                    let command = match mode { SubmitMode::Send => AgentCommand::Steer(user), SubmitMode::Queue => AgentCommand::FollowUp(user) };
                                    match active.commands.try_send(command) {
                                        Ok(()) => composer.accept_submission(),
                                        Err(error) => status = error.to_string(),
                                    }
                                } else if text.trim() == "/retry-pending" {
                                    if let Some(command) = pending.first().cloned() {
                                        match command {
                                            AgentCommand::Steer(user) | AgentCommand::FollowUp(user) => match session.start(user).await {
                                                Ok(active) => { run = Some(active); pending.remove(0); composer.accept_submission(); }
                                                Err(error) => status = safe_text(&error),
                                            },
                                            AgentCommand::Interrupt => { pending.remove(0); }
                                        }
                                    } else { status = "No unsent inputs.".into(); }
                                } else if text.trim() == "/quit" {
                                    break;
                                } else if text.starts_with('/') {
                                    match session.command(&text).await {
                                        Ok(effect) => {
                                            composer.accept_submission();
                                            match effect {
                                                CommandEffect::Notice(notice) => status = safe_text(&notice),
                                                CommandEffect::Reset(messages) => {
                                                    pending = session.pending_commands().map_err(io::Error::other)?;
                                                    tui.terminal.clear_visible_screen()?;
                                                    for message in messages { write_message(&mut tui.terminal, &message, &session.view().directory)?; }
                                                }
                                            }
                                        }
                                        Err(error) => status = safe_text(&error),
                                    }
                                } else {
                                    match session.start(user).await {
                                        Ok(active) => { run = Some(active); composer.accept_submission(); status = "Working…".into(); }
                                        Err(error) => status = safe_text(&error),
                                    }
                                }
                            }
                        }
                    }
                    Event::Paste(text) => {
                        let result = match &mut prompt { Some(pane) => pane.paste(&text), None => composer.paste(&text) };
                        if let Err(error) = result { status = error.into(); }
                    },
                    Event::Resize(_, _) => tui.terminal.autoresize()?,
                    Event::FocusGained => tui.terminal.invalidate_viewport(),
                    Event::FocusLost | Event::Mouse(_) => {}
                }
            }
        }
    }
    if let Some(active) = &run {
        let _ = active.commands.try_send(AgentCommand::Interrupt);
    }
    Ok(())
}

async fn next_agent_event(run: &mut Option<AgentRun>) -> Option<AgentEvent> {
    match run {
        Some(run) => run.events.recv().await,
        None => std::future::pending().await,
    }
}

async fn next_input_event<E: futures::Stream<Item = io::Result<Event>> + Unpin>(
    input: &mut E,
    closing: bool,
) -> Option<io::Result<Event>> {
    if closing {
        std::future::pending().await
    } else {
        input.next().await
    }
}

#[cfg(test)]
#[path = "frontend_tests.rs"]
mod tests;
