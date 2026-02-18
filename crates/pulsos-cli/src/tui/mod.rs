//! Live-updating TUI dashboard for `pulsos status --watch`.
//!
//! Architecture:
//! - Background poller (tokio task) fetches platform data on intervals
//! - Crossterm reader (OS thread) captures keyboard/resize events
//! - Tick generator (tokio task) drives UI refresh at configured FPS
//! - Main loop renders via ratatui and processes events

pub mod app;
pub mod event;
pub mod keys;
pub mod poll;
pub mod render;
pub mod terminal;
pub mod theme;
pub mod widgets;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self as ct_event, Event as CtEvent};
use tokio::sync::{mpsc, watch};

use pulsos_core::config::types::PulsosConfig;

use self::app::{App, DataSnapshot};
use self::event::AppEvent;
use self::theme::Theme;

/// Run the live-updating TUI dashboard.
///
/// This is the main entry point called from `pulsos status --watch`.
/// It sets up the terminal, spawns background tasks, and runs the event loop.
pub async fn run_tui(config: PulsosConfig) -> Result<()> {
    // Install panic hook that restores the terminal before printing the panic.
    terminal::install_panic_hook();

    // Set up the terminal.
    let mut term = terminal::setup()?;

    // Resolve theme from config + environment.
    let theme = Theme::resolve(&config.tui.theme);

    // Create the data channel (poller → main loop).
    let initial_snapshot = DataSnapshot::default();
    let (data_tx, mut data_rx) = watch::channel(initial_snapshot.clone());

    // Create force-refresh channel (main loop → poller).
    let (force_tx, force_rx) = mpsc::channel::<()>(1);

    // Create event channel (crossterm reader + tick → main loop).
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(64);

    // Spawn the background data poller.
    let poller_config = config.clone();
    tokio::spawn(async move {
        poll::run_poller(poller_config, data_tx, force_rx).await;
    });

    // Spawn the tick generator.
    let fps = config.tui.fps.max(1);
    let tick_interval = Duration::from_millis(1000 / fps);
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
    let mut app = App::new(initial_snapshot, config.tui.clone());

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
        }

        // Render the current frame.
        term.draw(|frame| {
            render::draw(frame, &app, &theme);
        })?;

        // Wait for the next event.
        if let Some(event) = event_rx.recv().await {
            match event {
                AppEvent::Key(key) => {
                    // Handle force refresh: if the key handler sets the flag,
                    // send a signal to the poller.
                    keys::handle_key(&mut app, key);
                    if app.force_refresh {
                        let _ = force_tx.try_send(());
                        app.force_refresh = false;
                    }
                }
                AppEvent::Tick => {
                    // Nothing to do — the render above already refreshes the UI.
                }
                AppEvent::Resize(w, h) => {
                    app.terminal_size = (w, h);
                }
            }
        }

        // Check quit flag.
        if app.should_quit {
            break;
        }
    }

    // Restore the terminal.
    terminal::teardown(&mut term)?;

    Ok(())
}
