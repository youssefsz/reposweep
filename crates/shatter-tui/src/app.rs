use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use shatter_core::{
    CancellationToken, ConfigService, DeleteRequest, DeleteService, FileConfigStore,
    FsDeletionBackend, ProtectionPolicy, ScanRequest, ScanScope, ScanService, SizeMode,
};
use tracing::warn;

use crate::state::{AppModel, HomeMode, Screen, handle_scan_event};
use crate::{storage, ui};

pub fn run(initial_path: Option<PathBuf>) -> shatter_core::Result<()> {
    let mut app_state = storage::load();
    let mut model = AppModel::new(initial_path, &app_state);

    enable_raw_mode()
        .map_err(|error| shatter_core::ShatterError::Config(format!("raw mode failed: {error}")))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|error| {
        shatter_core::ShatterError::Config(format!("alternate screen failed: {error}"))
    })?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|error| {
        shatter_core::ShatterError::Config(format!("failed to create terminal: {error}"))
    })?;

    let run_result = run_loop(&mut terminal, &mut model, &mut app_state);
    let restore_result = restore_terminal(&mut terminal);
    let save_result = storage::save(&app_state);

    run_result?;
    restore_result?;
    save_result?;
    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    model: &mut AppModel,
    app_state: &mut storage::AppState,
) -> shatter_core::Result<()> {
    loop {
        model.tick = model.tick.saturating_add(1);
        drain_background_messages(model);

        terminal
            .draw(|frame| ui::render(frame, model))
            .map_err(|error| {
                shatter_core::ShatterError::Config(format!("terminal draw failed: {error}"))
            })?;

        if model.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(120)).map_err(|error| {
            shatter_core::ShatterError::Config(format!("event poll failed: {error}"))
        })? {
            let event = event::read().map_err(|error| {
                shatter_core::ShatterError::Config(format!("event read failed: {error}"))
            })?;
            if let Event::Key(key) = event {
                if key.kind == KeyEventKind::Press {
                    handle_key(model, app_state, key.code, key.modifiers)?;
                }
            }
        }
    }
    Ok(())
}

fn handle_key(
    model: &mut AppModel,
    app_state: &mut storage::AppState,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> shatter_core::Result<()> {
    if model.summary.is_some() {
        return handle_summary_key(model, app_state, code);
    }

    if model.delete.in_progress {
        return handle_delete_progress_key(model, code);
    }

    if model
        .results
        .as_ref()
        .and_then(|results| results.pending_delete.as_ref())
        .is_some()
    {
        return handle_confirm_key(model, code);
    }

    match model.screen {
        Screen::Home | Screen::Error => handle_home_key(model, app_state, code, modifiers),
        Screen::Scanning => {
            match code {
                KeyCode::Char('c') | KeyCode::Esc => {
                    if let Some(cancel) = &model.scan.cancel {
                        cancel.cancel();
                    }
                }
                KeyCode::Char('q') => {
                    if let Some(cancel) = &model.scan.cancel {
                        cancel.cancel();
                    }
                    model.should_quit = true;
                }
                _ => {}
            }
            Ok(())
        }
        Screen::Results => handle_results_key(model, app_state, code),
    }
}

fn handle_home_key(
    model: &mut AppModel,
    app_state: &mut storage::AppState,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> shatter_core::Result<()> {
    match code {
        KeyCode::Esc => {
            if model.last_error.is_some() {
                model.clear_error();
            } else {
                model.should_quit = true;
            }
        }
        KeyCode::Up => model.move_home_selection(-1),
        KeyCode::Down => model.move_home_selection(1),
        KeyCode::Tab => {
            if model.home.mode == HomeMode::PathEntry {
                model.switch_home_mode(HomeMode::Browser);
            } else {
                model.switch_home_mode(HomeMode::PathEntry);
            }
        }
        KeyCode::Enter => match model.home.mode {
            HomeMode::PathEntry => start_scan_from_input(model, app_state)?,
            HomeMode::Browser => model.enter_browser(),
        },
        KeyCode::Char('s') if model.home.mode == HomeMode::Browser => {
            model.home.input = model.home.browser_path.display().to_string();
            start_scan_from_input(model, app_state)?;
        }
        KeyCode::Char('q') if model.home.mode == HomeMode::Browser => {
            model.should_quit = true;
        }
        KeyCode::Backspace if model.home.mode == HomeMode::PathEntry => {
            model.home.input.pop();
        }
        KeyCode::Char(character) if model.home.mode == HomeMode::PathEntry => {
            if modifiers.contains(KeyModifiers::CONTROL) && character == 'c' {
                model.should_quit = true;
                return Ok(());
            } else if modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(());
            }
            model.home.input.push(character);
        }
        _ => {}
    }
    Ok(())
}

fn handle_results_key(
    model: &mut AppModel,
    app_state: &mut storage::AppState,
    code: KeyCode,
) -> shatter_core::Result<()> {
    let Some(results) = &mut model.results else {
        return Ok(());
    };

    match code {
        KeyCode::Char('q') => model.should_quit = true,
        KeyCode::Esc | KeyCode::Char('h') => {
            model.summary = None;
            model.screen = Screen::Home;
        }
        KeyCode::Up => results.move_selection(-1),
        KeyCode::Down => results.move_selection(1),
        KeyCode::Char(' ') => results.toggle_selected(),
        KeyCode::Char('a') => results.toggle_all_visible(),
        KeyCode::Char('n') => results.clear_selection(),
        KeyCode::Char('f') => results.cycle_filter(),
        KeyCode::Char('s') => results.cycle_sort(),
        KeyCode::Enter | KeyCode::Delete | KeyCode::Char('d') | KeyCode::Char('x') => {
            results.begin_delete(shatter_core::DeleteStrategy::Trash);
        }
        KeyCode::Char('D') => {
            results.begin_delete(shatter_core::DeleteStrategy::Permanent);
        }
        KeyCode::Char('r') => start_scan_from_input(model, app_state)?,
        _ => {}
    }

    Ok(())
}

fn handle_confirm_key(model: &mut AppModel, code: KeyCode) -> shatter_core::Result<()> {
    let Some(results) = &model.results else {
        return Ok(());
    };

    match code {
        KeyCode::Esc | KeyCode::Char('n') => {
            if let Some(results) = &mut model.results {
                results.pending_delete = None;
            }
        }
        KeyCode::Char('q') => model.should_quit = true,
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
            let strategy = results
                .pending_delete
                .as_ref()
                .map(|pending| pending.strategy)
                .unwrap_or(shatter_core::DeleteStrategy::Trash);
            let items = results.delete_items();
            spawn_delete(model, items, strategy);
        }
        _ => {}
    }

    Ok(())
}

fn handle_delete_progress_key(model: &mut AppModel, code: KeyCode) -> shatter_core::Result<()> {
    if matches!(code, KeyCode::Char('q')) {
        model.should_quit = true;
    }
    Ok(())
}

fn handle_summary_key(
    model: &mut AppModel,
    app_state: &mut storage::AppState,
    code: KeyCode,
) -> shatter_core::Result<()> {
    match code {
        KeyCode::Char('q') => model.should_quit = true,
        KeyCode::Enter | KeyCode::Esc => {
            model.summary = None;
        }
        KeyCode::Char('h') => {
            model.summary = None;
            model.screen = Screen::Home;
        }
        KeyCode::Char('r') => {
            model.summary = None;
            start_scan_from_input(model, app_state)?;
        }
        _ => {}
    }
    Ok(())
}

fn start_scan_from_input(
    model: &mut AppModel,
    app_state: &mut storage::AppState,
) -> shatter_core::Result<()> {
    let root = PathBuf::from(model.home.input.trim());
    if root.as_os_str().is_empty() {
        model.set_error("Enter a path to scan.");
        return Ok(());
    }

    if !root.exists() || !root.is_dir() {
        model.set_error(format!(
            "Path is not a readable directory: {}",
            root.display()
        ));
        return Ok(());
    }

    model.home.browser_path = root.clone();
    model.refresh_browser();
    model.remember_recent_path(&root);
    app_state.recent_paths = model.home.recent_paths.clone();
    app_state.last_browser_path = Some(root.clone());
    storage::save(app_state)?;

    spawn_scan(model, root);
    Ok(())
}

fn spawn_scan(model: &mut AppModel, root: PathBuf) {
    let (event_tx, event_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let cancel = CancellationToken::new();
    let thread_cancel = cancel.clone();
    let thread_root = root.clone();

    thread::spawn(move || {
        let config = ConfigService::new(FileConfigStore).load_or_default();
        let service = ScanService::from_config(config);
        let result = service.scan(
            ScanRequest {
                roots: vec![thread_root],
                scope: ScanScope::All,
                age_filter: None,
                protection_policy: ProtectionPolicy::RespectConfig,
                size_mode: SizeMode::Accurate,
            },
            Some(event_tx),
            thread_cancel,
        );
        let _ = result_tx.send(result);
    });

    model.begin_scan(root, event_rx, result_rx, cancel);
}

fn spawn_delete(
    model: &mut AppModel,
    items: Vec<shatter_core::ScanItem>,
    strategy: shatter_core::DeleteStrategy,
) {
    let item_count = items.len();
    let (result_tx, result_rx) = mpsc::channel();

    thread::spawn(move || {
        let result =
            DeleteService::new(FsDeletionBackend).delete(DeleteRequest { items, strategy });
        let _ = result_tx.send(result);
    });

    model.delete.in_progress = true;
    model.delete.item_count = item_count;
    model.delete.strategy = strategy;
    model.delete.result_rx = Some(result_rx);
    if let Some(results) = &mut model.results {
        results.pending_delete = None;
    }
}

fn drain_background_messages(model: &mut AppModel) {
    if matches!(model.screen, Screen::Scanning) {
        model.scan.stalled_ticks = model.scan.stalled_ticks.saturating_add(1);
    }

    loop {
        let event = {
            let Some(receiver) = model.scan.event_rx.as_ref() else {
                break;
            };
            receiver.try_recv()
        };
        match event {
            Ok(event) => handle_scan_event(model, event),
            Err(mpsc::TryRecvError::Empty) | Err(mpsc::TryRecvError::Disconnected) => break,
        }
    }

    if let Some(receiver) = model.scan.result_rx.take() {
        match receiver.try_recv() {
            Ok(Ok(report)) => model.finish_scan(report),
            Ok(Err(error)) => {
                model.set_error(error.to_string());
                model.scan = crate::state::ScanState::default();
            }
            Err(mpsc::TryRecvError::Empty) => {
                model.scan.result_rx = Some(receiver);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                warn!("scan result channel disconnected");
            }
        }
    }

    if let Some(receiver) = model.delete.result_rx.take() {
        match receiver.try_recv() {
            Ok(result) => model.finish_delete(result, model.delete.strategy),
            Err(mpsc::TryRecvError::Empty) => {
                model.delete.result_rx = Some(receiver);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                warn!("delete result channel disconnected");
                model.delete = crate::state::DeleteState::default();
            }
        }
    }
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> shatter_core::Result<()> {
    disable_raw_mode().map_err(|error| {
        shatter_core::ShatterError::Config(format!("disable raw mode failed: {error}"))
    })?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|error| {
        shatter_core::ShatterError::Config(format!("leave alternate screen failed: {error}"))
    })?;
    terminal
        .show_cursor()
        .map_err(|error| shatter_core::ShatterError::Config(format!("show cursor failed: {error}")))
}
