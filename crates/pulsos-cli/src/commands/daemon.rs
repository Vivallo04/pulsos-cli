//! `pulsos daemon [run|start|stop|status]` — persistent background daemon.
//!
//! Architecture:
//! - `run` — foreground: starts the tray icon on the main thread and
//!   spawns a Tokio runtime for the engine + SSE server.
//! - `start` — background: re-execs `daemon run` as a detached process.
//! - `stop` — reads the PID file and sends SIGTERM.
//! - `status` — checks the `/health` endpoint and prints the port.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::Context;
use clap::{Args, Subcommand};

use pulsos_core::config::types::PulsosConfig;

use crate::daemon::engine::run_engine;
use crate::daemon::notify::NotificationState;
use crate::daemon::server::{start_server, DaemonStateEvent};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Debug, Subcommand)]
pub enum DaemonAction {
    /// Run the daemon in the foreground (owns the main thread for the tray icon).
    Run,
    /// Start the daemon as a detached background process.
    Start,
    /// Stop the running daemon (sends SIGTERM via the PID file).
    Stop,
    /// Print the daemon's running state and port.
    Status,
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pulsos")
}

fn pid_path() -> PathBuf {
    config_dir().join("daemon.pid")
}

fn port_path() -> PathBuf {
    config_dir().join("daemon.port")
}

fn read_daemon_port() -> Option<u16> {
    std::fs::read_to_string(port_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

// ──────────────────────────────────────────────────────────────────────────────
// `run` — must be called BEFORE entering the Tokio runtime (owns main thread)
// ──────────────────────────────────────────────────────────────────────────────

/// Entry point for `daemon run`. Called from `main()` *before* constructing
/// a Tokio runtime so that the main thread is available for the tray event loop.
pub fn run_daemon_main_thread(config: PulsosConfig) -> anyhow::Result<()> {
    let pid_file = pid_path();
    std::fs::create_dir_all(config_dir())?;
    std::fs::write(&pid_file, std::process::id().to_string())?;

    // Broadcast channel: engine → SSE subscribers & tray state updater.
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<DaemonStateEvent>(64);

    // Spawn Tokio runtime on a background thread.
    let engine_tx = broadcast_tx.clone();
    let engine_config = config.clone();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build Tokio runtime")
            .block_on(async move {
                if let Err(e) = start_server(engine_tx.clone()).await {
                    eprintln!("daemon: SSE server failed to start: {e}");
                }
                run_engine(engine_config, engine_tx).await;
            });
    });

    // Platforms with tray support drive their UI on the main thread.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    run_tray_event_loop(broadcast_tx, pid_file)?;

    // On unsupported platforms, just park the main thread until killed.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        println!("Daemon running (no tray icon on this platform). Press Ctrl-C to stop.");
        std::thread::park();
    }

    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run_tray_event_loop(
    broadcast_tx: tokio::sync::broadcast::Sender<DaemonStateEvent>,
    pid_file: PathBuf,
) -> anyhow::Result<()> {
    use crate::daemon::tray::{
        compute_tray_state, launch_terminal_tui, menu_event_receiver, TrayManager,
    };
    use tao::event_loop::{ControlFlow, EventLoopBuilder};

    let tray_manager = TrayManager::new()?;
    let menu_channel = menu_event_receiver();
    let mut notifier = NotificationState::new();
    let mut tray_rx = broadcast_tx.subscribe();

    let event_loop = EventLoopBuilder::<()>::with_user_event().build();
    let quit_id = tray_manager.quit_id.clone();
    let open_id = tray_manager.open_id.clone();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Drain any new snapshots from the engine and update the tray.
        while let Ok(ev) = tray_rx.try_recv() {
            notifier.check_and_notify(&ev.snapshot);
            tray_manager.set_state(&compute_tray_state(&ev.snapshot));
        }

        // Handle menu events.
        while let Ok(menu_event) = menu_channel.try_recv() {
            if menu_event.id == quit_id {
                let _ = std::fs::remove_file(&pid_file);
                let _ = std::fs::remove_file(port_path());
                *control_flow = ControlFlow::Exit;
            } else if menu_event.id == open_id {
                launch_terminal_tui();
            }
        }
    });
}

// ──────────────────────────────────────────────────────────────────────────────
// `start` / `stop` / `status` — async, called inside the normal Tokio runtime
// ──────────────────────────────────────────────────────────────────────────────

pub async fn execute(
    args: DaemonArgs,
    config_path: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    let _ = config_path; // config loaded in main for `run`; unused for other actions
    match args.action {
        DaemonAction::Run => {
            // Should never reach here — `run` is intercepted in main() before
            // the Tokio runtime is created.
            anyhow::bail!("`daemon run` must be called before entering the Tokio runtime");
        }

        DaemonAction::Start => {
            let bin = std::env::current_exe().context("cannot determine own executable path")?;
            std::process::Command::new(bin)
                .arg("daemon")
                .arg("run")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("failed to spawn daemon process")?;
            println!("Daemon started in the background.");
        }

        DaemonAction::Stop => {
            let pid_str = std::fs::read_to_string(pid_path())
                .context("daemon PID file not found — is the daemon running?")?;
            let pid: u32 = pid_str.trim().parse().context("invalid PID file")?;
            #[cfg(unix)]
            {
                std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status()
                    .context("failed to send SIGTERM to daemon")?;
                let _ = std::fs::remove_file(pid_path());
                let _ = std::fs::remove_file(port_path());
                println!("Daemon (PID {pid}) stopped.");
            }
            #[cfg(windows)]
            {
                std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .status()
                    .context("failed to terminate daemon process")?;
                let _ = std::fs::remove_file(pid_path());
                let _ = std::fs::remove_file(port_path());
                println!("Daemon (PID {pid}) stopped.");
            }
        }

        DaemonAction::Status => {
            let Some(port) = read_daemon_port() else {
                println!("Daemon is not running (no port file found).");
                return Ok(());
            };
            let url = format!("http://127.0.0.1:{port}/health");
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Daemon is running on port {port}.");
                }
                _ => {
                    println!("Daemon port file exists (port {port}) but health check failed.");
                }
            }
        }
    }
    Ok(())
}
