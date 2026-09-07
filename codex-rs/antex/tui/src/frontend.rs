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
use crate::composer::ComposerAction;
use crate::composer::SubmitMode;
use crate::foreground::InputState;
use crate::frontend_layout::draw;
use crate::insert_history::insert_history_lines;
use crate::keymap::RuntimeKeymap;
use crate::terminal_guard::TerminalGuard;
use crate::transcript::safe_text;
use crate::transcript::write_message;
use crate::tui::Tui;

enum CommandOrigin {
    Composer,
    Picker,
}

pub async fn run(session: &mut impl Session, settings: crate::Settings) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "interactive mode requires a terminal; use antex exec for pipes",
        ));
    }
    for warning in &settings.warnings {
        eprintln!("antex: {warning}");
    }
    if settings.theme.is_some()
        && let Some(warning) = crate::render::highlight::set_theme_override(
            settings.theme.clone(),
            settings.home.clone(),
        )
    {
        eprintln!("antex: {warning}");
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
    run_terminal(&mut tui, session, EventStream::new(), settings).await
}

async fn run_terminal<B, E>(
    tui: &mut Tui<B>,
    session: &mut impl Session,
    mut input: E,
    settings: crate::Settings,
) -> io::Result<()>
where
    B: ratatui::backend::Backend<Error = io::Error> + io::Write,
    E: futures::Stream<Item = io::Result<Event>> + Unpin,
{
    let mut composer = Composer::new(Arc::new(RuntimeKeymap::defaults()));
    composer.configure(&settings);
    crate::presentation_commands::restore(&settings, session).map_err(io::Error::other)?;
    composer
        .load_stash(
            session
                .load_ui_state("promptStash")
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
    let mut run: Option<AgentRun> = None;
    let mut turn_ready = false;
    let mut status = String::new();
    let mut live = String::new();
    let mut last_response = String::new();
    let mut _clipboard_lease = None;
    let mut pending = session.pending_commands().map_err(io::Error::other)?;
    let mut prompt = None;
    let mut input_state = InputState::Open;
    let mut buffered_input = std::collections::VecDeque::new();
    let frames = crate::tui::FrameRequester::new();
    let mut startup = Some(crate::startup::Startup::new(&settings, frames.clone()));
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
        if let Some(text) = crate::transcript::assistant_text(&message) {
            last_response = text;
        }
        write_message(&mut tui.terminal, &message, &session.view().directory)?;
    }
    let frame_budget = std::time::Duration::from_millis(33);
    let mut last_draw = std::time::Instant::now();
    let mut force_draw = true;
    let mut ui_command: Option<(String, CommandOrigin)> = None;
    loop {
        if input_state == InputState::Closed && run.is_none() {
            break;
        }
        if let Some((command, origin)) = ui_command.take() {
            status = "Loading… · Esc cancels".into();
            draw(
                tui,
                &mut composer,
                session,
                &status,
                &live,
                &mut prompt,
                &startup,
            )?;
            let result = match crate::presentation_commands::handle(&command, &settings, session) {
                Some(result) => result,
                None => {
                    crate::foreground::wait(
                        session.command(&command),
                        &mut input,
                        &mut buffered_input,
                        &mut input_state,
                    )
                    .await
                }
            };
            match result {
                Err(error) => status = safe_text(&error),
                Ok(effect) => {
                    if matches!(origin, CommandOrigin::Composer) {
                        composer.accept_submission();
                    }
                    match effect {
                        CommandEffect::Page(page) => match crate::pager::Pager::new(
                            page,
                            session.view().directory,
                            settings.keymap.clone(),
                        ) {
                            Ok(pager) => prompt = Some(crate::overlay::Overlay::Pager(pager)),
                            Err(error) => status = safe_text(&error),
                        },
                        CommandEffect::Notice(notice) => status = safe_text(&notice),
                        CommandEffect::Image(image) => {
                            status = match attach_image(&mut composer, image) {
                                Ok(()) => "Image attached".into(),
                                Err(error) => safe_text(&error),
                            }
                        }
                        CommandEffect::Picker(spec) => {
                            match crate::picker::Picker::new(spec, settings.keymap.clone()) {
                                Ok(picker) => {
                                    prompt = Some(crate::overlay::Overlay::Picker(picker))
                                }
                                Err(error) => status = safe_text(&error),
                            }
                        }
                        CommandEffect::Reset(messages) => {
                            crate::presentation_commands::restore(&settings, session)
                                .map_err(io::Error::other)?;
                            last_response.clear();
                            pending = session.pending_commands().map_err(io::Error::other)?;
                            composer
                                .load_stash(
                                    session
                                        .load_ui_state("promptStash")
                                        .map_err(io::Error::other)?,
                                )
                                .map_err(io::Error::other)?;
                            tui.terminal.clear_visible_screen()?;
                            for message in messages {
                                if let Some(text) = crate::transcript::assistant_text(&message) {
                                    last_response = text;
                                }
                                write_message(
                                    &mut tui.terminal,
                                    &message,
                                    &session.view().directory,
                                )?;
                            }
                        }
                    }
                }
            }
            force_draw = true;
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
                let Some(event) = event else { run = None; turn_ready = false; continue; };
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
                        if matches!(message, antex_core::Message::User(_)) { turn_ready = true; }
                        if let Some(text) = crate::transcript::assistant_text(&message) { last_response = text; }
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
                        prompt = Some(crate::overlay::Overlay::Prompt(pane));
                        status = "Waiting for your answer".into();
                    }
                    AgentEvent::Error(error) => status = safe_text(&error.to_string()),
                    AgentEvent::Finished { reason, .. } => {
                        pending = session.pending_commands().map_err(io::Error::other)?;
                        status = format!("{reason:?}; {} unsent inputs retained", pending.len());
                        run = None;
                        turn_ready = false;
                        prompt = None;
                        live.clear();
                    }
                    AgentEvent::ContextCheckpoint(_) => status = "Context compacted".into(),
                    AgentEvent::Usage(usage) => status = format!("{} input · {} output tokens", usage.input_tokens, usage.output_tokens),
                    AgentEvent::Quota(_) | AgentEvent::ReasoningDelta(_) | AgentEvent::TurnCompleted => {}
                }
            }
            event = next_input_event(&mut input, input_state, &mut buffered_input) => {
                let Some(event) = event else {
                    input_state = InputState::Closed;
                    if let Some(active) = &run { let _ = active.commands.try_send(AgentCommand::Interrupt); }
                    continue;
                };
                force_draw = true;
                match event? {
                    Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Release => continue,
                    Event::Key(key) if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') => {
                        if let Some(active) = &run { let _ = active.commands.try_send(AgentCommand::Interrupt); }
                        else if prompt.take().is_none() { break; }
                    }
                    Event::Key(key) => {
                        if let Some(pane) = &mut prompt {
                            match pane.key(key) {
                                Ok(crate::overlay::OverlayAction::Close) => prompt = None,
                                Ok(crate::overlay::OverlayAction::Continue) => {},
                                Ok(crate::overlay::OverlayAction::Command(command)) => { prompt = None; ui_command = Some((command, CommandOrigin::Picker)); },
                                Err(error) => status = safe_text(&error),
                            }
                            continue;
                        }
                        if crate::key_hint::ctrl(KeyCode::Char('s')).is_press(key) && !composer.chord_pending() {
                            match composer.toggle_stash(|value| session.save_ui_state("promptStash", value)) {
                                Ok(()) => status = if composer.has_stash() { "Prompt stashed · Ctrl+S restores it" } else { "Prompt restored" }.into(),
                                Err(error) => status = safe_text(&error),
                            }
                            continue;
                        }
                        if crate::key_hint::ctrl(KeyCode::Char('v')).is_press(key) || crate::key_hint::ctrl_alt(KeyCode::Char('v')).is_press(key) {
                            let result = match crate::clipboard_paste::image_source() {
                                Ok(source) => crate::foreground::wait(session.prepare_image(source), &mut input, &mut buffered_input, &mut input_state).await.and_then(|image| attach_image(&mut composer, image)),
                                Err(error) => Err(error),
                            };
                            status = match result { Ok(()) => "Image attached".into(), Err(error) => safe_text(&error) };
                            continue;
                        }
                        match composer.key(key) {
                            Err(error) => status = error.into(),
                            Ok(None) => {},
                            Ok(Some(ComposerAction::Transcript)) => ui_command = Some(("/transcript".into(), CommandOrigin::Picker)),
                            Ok(Some(ComposerAction::Clear)) => tui.terminal.clear_visible_screen()?,
                            Ok(Some(ComposerAction::Copy)) => match copy_response(&last_response) {
                                Ok(lease) => { _clipboard_lease = lease; status = "Copied last response".into(); }
                                Err(error) => status = safe_text(&error),
                            },
                            Ok(Some(ComposerAction::Submit(mode))) => {
                                let draft = composer.draft();
                                let user = draft.input();
                                let text = user.content.iter().filter_map(|content| match content { antex_core::Content::Text(text) => Some(text.as_str()), _ => None }).collect::<String>();
                                if user.content.is_empty() { continue; }
                                if let Some(active) = &run {
                                    if !turn_ready { status = "The turn is starting; your draft is retained.".into(); continue; }
                                    if !pending.is_empty() {
                                        status = "Recover the remaining unsent inputs before queueing more.".into();
                                        continue;
                                    }
                                    let command = match mode { SubmitMode::Send => AgentCommand::Steer(user), SubmitMode::Queue => AgentCommand::FollowUp(user) };
                                    if let Err(error) = session.queue_command(&command) { status = safe_text(&error); continue; }
                                    match active.commands.try_send(command) {
                                        Ok(()) => composer.accept_submission(),
                                        Err(error) => {
                                            pending = session.pending_commands().map_err(io::Error::other)?;
                                            status = error.to_string();
                                        }
                                    }
                                } else if text.trim() == "/retry-pending" {
                                    if let Some(command) = pending.first().cloned() {
                                        match command {
                                            AgentCommand::Steer(user) | AgentCommand::FollowUp(user) => match crate::foreground::wait(session.start(user), &mut input, &mut buffered_input, &mut input_state).await {
                                                Ok(active) => { run = Some(active); turn_ready = false; pending.remove(0); composer.accept_submission(); }
                                                Err(error) => status = safe_text(&error),
                                            },
                                            AgentCommand::Interrupt => { pending.remove(0); }
                                        }
                                    } else { status = "No unsent inputs.".into(); }
                                } else if text.trim() == "/quit" {
                                    break;
                                } else if text.trim() == "/copy" {
                                    match copy_response(&last_response) {
                                        Ok(lease) => { _clipboard_lease = lease; composer.accept_submission(); status = "Copied last response".into(); }
                                        Err(error) => status = safe_text(&error),
                                    }
                                } else if text.starts_with('/') {
                                    if user.content.iter().any(|content| matches!(content, antex_core::Content::Image { .. })) {
                                        status = "Commands cannot include image attachments; stash the draft first.".into();
                                    } else {
                                        ui_command = Some((text, CommandOrigin::Composer));
                                    }
                                } else {
                                    match crate::foreground::wait(session.start(user), &mut input, &mut buffered_input, &mut input_state).await {
                                        Ok(active) => { run = Some(active); turn_ready = false; composer.accept_submission(); status = "Working…".into(); }
                                        Err(error) => status = safe_text(&error),
                                    }
                                }
                            }
                        }
                        if let Some(text) = composer.take_clipboard_yank() {
                            match crate::clipboard_copy::copy_to_clipboard(&text, crate::clipboard_copy::CopyFormat::PlainText) {
                                Ok(lease) => _clipboard_lease = lease,
                                Err(error) => status = safe_text(&error),
                            }
                        }
                    }
                    Event::Paste(text) => {
                        if prompt.is_none() && let Some(path) = crate::clipboard_paste::pasted_image_path(&text) {
                            match crate::foreground::wait(session.prepare_image(crate::ImageSource::File(path)), &mut input, &mut buffered_input, &mut input_state).await.and_then(|image| attach_image(&mut composer, image)) {
                                Ok(()) => { status = "Image attached".into(); continue; }
                                Err(error) => status = safe_text(&error),
                            }
                        }
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

fn copy_response(text: &str) -> Result<Option<crate::clipboard_copy::ClipboardLease>, String> {
    if text.is_empty() {
        return Err("No assistant response to copy.".into());
    }
    crate::clipboard_copy::copy_to_clipboard(text, crate::clipboard_copy::CopyFormat::Markdown)
}

fn attach_image(composer: &mut Composer, image: antex_core::Content) -> Result<(), String> {
    match image {
        antex_core::Content::Image { media_type, data } => {
            composer.attach(media_type, data).map_err(str::to_owned)
        }
        _ => Err("Image preparation returned non-image content.".into()),
    }
}

async fn next_input_event<E: futures::Stream<Item = io::Result<Event>> + Unpin>(
    input: &mut E,
    state: InputState,
    buffered: &mut std::collections::VecDeque<io::Result<Event>>,
) -> Option<io::Result<Event>> {
    if let Some(event) = buffered.pop_front() {
        return Some(event);
    }
    if state == InputState::Closed {
        std::future::pending().await
    } else {
        input.next().await
    }
}

#[cfg(test)]
#[path = "frontend_tests.rs"]
mod tests;
