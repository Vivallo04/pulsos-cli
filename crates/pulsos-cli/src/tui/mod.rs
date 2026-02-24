//! Live-updating TUI dashboard for `pulsos status --watch`.
//!
//! Architecture:
//! - Background poller (tokio task) fetches platform data on intervals
//! - Crossterm reader (OS thread) captures keyboard/resize events
//! - Tick generator (tokio task) drives UI refresh at configured FPS
//! - Main loop renders via ratatui and processes events

pub mod actions;
pub mod app;
pub mod event;
pub mod keys;
pub mod log_buffer;
pub mod poll;
pub mod render;
pub mod settings_flow;
pub mod terminal;
pub mod theme;
mod visual_check;
pub mod widgets;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self as ct_event, Event as CtEvent};
use tokio::sync::{mpsc, watch};

use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::PulsosConfig;

use self::actions::{ActionRequest, ActionResult};
use self::app::{App, DataSnapshot, Tab};
use self::event::AppEvent;
use self::log_buffer::{LogRingBuffer, TuiActiveFlag};
use self::poll::PollerCommand;

/// Run the live-updating TUI dashboard.
///
/// This is the main entry point called from `pulsos status --watch`.
/// It sets up the terminal, spawns background tasks, and runs the event loop.
pub async fn run_tui(
    config: PulsosConfig,
    config_path: Option<PathBuf>,
    log_buffer: LogRingBuffer,
    tui_active: TuiActiveFlag,
) -> Result<()> {
    // Install panic hook that restores the terminal before printing the panic.
    terminal::install_panic_hook(Some(tui_active.clone()));

    // Suppress stderr while TUI is active; logs go to ring buffer only.
    tui_active.set_active(true);

    // Set up the terminal.
    let mut term = match terminal::setup() {
        Ok(t) => t,
        Err(e) => {
            tui_active.set_active(false);
            return Err(e);
        }
    };

    // Create the data channel (poller → main loop).
    let initial_snapshot = DataSnapshot::default();
    let (data_tx, mut data_rx) = watch::channel(initial_snapshot.clone());

    // Create poller command channel (main loop → poller).
    let (poller_tx, poller_rx) = mpsc::channel::<PollerCommand>(8);

    // Create event channel (crossterm reader + tick → main loop).
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(64);

    // Create settings action channels.
    let (action_req_tx, action_req_rx) = mpsc::channel::<ActionRequest>(8);
    let (action_result_tx, mut action_result_rx) = mpsc::channel::<ActionResult>(8);
    let cache = match CacheStore::open_or_temporary() {
        Ok(c) => std::sync::Arc::new(c),
        Err(e) => {
            // Terminal is already in raw mode; restore it before returning.
            tui_active.set_active(false);
            terminal::teardown(&mut term)?;
            return Err(e.into());
        }
    };

    // Spawn the background data poller (or connect to daemon if running).
    let poller_config = config.clone();
    let poller_cache = cache.clone();
    tokio::spawn(async move {
        poll::run_poller_or_daemon_client(poller_config, data_tx, poller_rx, poller_cache).await;
    });

    // Spawn settings action worker.
    let action_cache = cache.clone();
    tokio::spawn(async move {
        actions::run_worker(config_path, action_req_rx, action_result_tx, action_cache).await;
    });

    // Forward action results into the main AppEvent stream.
    let action_event_tx = event_tx.clone();
    tokio::spawn(async move {
        while let Some(result) = action_result_rx.recv().await {
            if action_event_tx
                .send(AppEvent::ActionResult(Box::new(result)))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Spawn the tick generator.
    let fps = config.tui.fps.max(1);
    let tick_interval = Duration::from_millis(1000 / fps);
    // Daemon status refresh is throttled to ~once per second (one full tick cycle).
    // Pre-loading the counter so the very first Settings visit triggers a check.
    let daemon_status_interval: u32 = fps as u32;
    let mut daemon_check_ticks: u32 = daemon_status_interval;
    let tick_tx = event_tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tick_interval);
        loop {
            interval.tick().await;
            if tick_tx.send(AppEvent::Tick).await.is_err() {
                break;
            }
        }
    });

    // Spawn the crossterm event reader in a dedicated OS thread.
    // (Crossterm's blocking poll must not run on the tokio runtime.)
    let input_tx = event_tx.clone();
    std::thread::spawn(move || {
        loop {
            // Poll with a 250ms timeout so the thread checks periodically
            // whether the channel is still open.
            if ct_event::poll(Duration::from_millis(250)).unwrap_or(false) {
                if let Ok(event) = ct_event::read() {
                    let app_event = match event {
                        CtEvent::Key(key) => Some(AppEvent::Key(key)),
                        CtEvent::Resize(w, h) => Some(AppEvent::Resize(w, h)),
                        _ => None,
                    };
                    if let Some(ev) = app_event {
                        if input_tx.blocking_send(ev).is_err() {
                            break; // receiver dropped — TUI exiting
                        }
                    }
                }
            }
        }
    });

    // Initialize the App state.
    let mut app = App::new(initial_snapshot, config.tui.clone(), log_buffer);

    // Get initial terminal size.
    if let Ok((w, h)) = crossterm::terminal::size() {
        app.terminal_size = (w, h);
    }

    // ── Main event loop ──
    loop {
        // Check for new data from the poller.
        if data_rx.has_changed().unwrap_or(false) {
            let snapshot = data_rx.borrow_and_update().clone();
            app.data = snapshot;
            app.clamp_selection();
            app.ensure_platform_tree_state();
            app.sync_platform_details_with_data();
        }

        // Render the current frame.
        term.draw(|frame| {
            render::draw(frame, &app, &app.theme);
        })?;

        // Wait for the next event.
        let Some(event) = event_rx.recv().await else {
            // All event senders dropped — exit gracefully.
            break;
        };

        match event {
            AppEvent::Key(key) => {
                // Handle force refresh: if the key handler sets the flag,
                // send a signal to the poller.
                keys::handle_key(&mut app, key);
                app.clamp_selection();
                app.ensure_platform_tree_state();
                app.sync_platform_details_with_data();
                if app.force_refresh {
                    let _ = poller_tx.try_send(PollerCommand::ForceRefresh);
                    app.force_refresh = false;
                }
                if let Some(request) = app.take_pending_action() {
                    if action_req_tx.try_send(request).is_err() {
                        app.settings_action_in_flight = false;
                        app.last_error = Some("Action queue is busy; try again.".to_string());
                    }
                }
            }
            AppEvent::Tick => {
                if app.active_tab == Tab::Settings {
                    daemon_check_ticks = daemon_check_ticks.saturating_add(1);
                    if daemon_check_ticks >= daemon_status_interval {
                        daemon_check_ticks = 0;
                        app.refresh_daemon_status();
                    }
                } else {
                    // Reset so the check fires immediately on the next Settings visit.
                    daemon_check_ticks = daemon_status_interval;
                }
            }
            AppEvent::Resize(w, h) => {
                app.terminal_size = (w, h);
            }
            AppEvent::ActionResult(result) => {
                let outcome = app.handle_action_result(*result);
                if let Some(config) = outcome.replace_config {
                    let _ = poller_tx.try_send(PollerCommand::ReplaceConfig(Box::new(config)));
                }
                if outcome.force_refresh {
                    let _ = poller_tx.try_send(PollerCommand::ForceRefresh);
                }
            }
        }

        // Check quit flag.
        if app.should_quit {
            break;
        }
    }

    // Re-enable stderr before restoring terminal.
    tui_active.set_active(false);

    // Restore the terminal.
    terminal::teardown(&mut term)?;

    Ok(())
}
