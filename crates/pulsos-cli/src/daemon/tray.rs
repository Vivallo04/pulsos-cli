//! Native system tray icon management (macOS Menu Bar / Windows tray).
//!
//! This module must only be compiled on platforms that support native tray icons.
//! Callers are responsible for ensuring this runs on the OS main thread.

use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::tui::app::DataSnapshot;
use pulsos_core::domain::deployment::DeploymentStatus;

pub struct TrayManager {
    pub tray: TrayIcon,
    pub quit_id: muda::MenuId,
    pub open_id: muda::MenuId,
}

pub enum TrayState {
    Neutral,
    Syncing,
    Alert(String),
}

impl TrayManager {
    pub fn new() -> anyhow::Result<Self> {
        let quit_item = MenuItem::new("Quit Pulsos", true, None);
        let open_item = MenuItem::new("View Dashboard", true, None);
        let quit_id = quit_item.id().clone();
        let open_id = open_item.id().clone();

        let menu = Menu::new();
        menu.append_items(&[&open_item, &PredefinedMenuItem::separator(), &quit_item])?;

        let icon = load_embedded_icon(include_bytes!("../../assets/icon-neutral.png"));
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Pulsos: Monitoring")
            .with_icon(icon)
            .build()?;

        Ok(Self {
            tray,
            quit_id,
            open_id,
        })
    }

    /// Update the tray icon and tooltip to reflect the current state.
    pub fn set_state(&self, state: &TrayState) {
        match state {
            TrayState::Neutral => {
                let icon = load_embedded_icon(include_bytes!("../../assets/icon-neutral.png"));
                self.tray.set_icon(Some(icon)).ok();
                self.tray
                    .set_tooltip(Some("Pulsos: All Systems Operational"))
                    .ok();
            }
            TrayState::Syncing => {
                let icon = load_embedded_icon(include_bytes!("../../assets/icon-sync.png"));
                self.tray.set_icon(Some(icon)).ok();
                self.tray.set_tooltip(Some("Pulsos: Syncing\u{2026}")).ok();
            }
            TrayState::Alert(msg) => {
                let icon = load_embedded_icon(include_bytes!("../../assets/icon-alert.png"));
                self.tray.set_icon(Some(icon)).ok();
                self.tray.set_tooltip(Some(msg.as_str())).ok();
            }
        }
    }
}

/// Compute the appropriate tray state from a snapshot.
pub fn compute_tray_state(snapshot: &DataSnapshot) -> TrayState {
    if snapshot.is_syncing {
        return TrayState::Syncing;
    }

    // Any endpoint currently down?
    for (name, tel) in &snapshot.telemetry {
        if let Some(ping) = tel.latest_ping() {
            if !ping.is_up {
                return TrayState::Alert(format!("\u{1f6a8} {} is DOWN", ping.url));
            }
        }
        let _ = name; // suppress lint
    }

    // Any main-branch deployment failure?
    let main_failed = snapshot.correlated.iter().any(|c| {
        let on_main = c
            .github
            .as_ref()
            .and_then(|g| g.branch.as_deref())
            .map(|b| b == "main" || b == "master")
            .unwrap_or(false);
        let failed = c
            .github
            .as_ref()
            .map(|g| g.status == DeploymentStatus::Failed)
            .unwrap_or(false);
        on_main && failed
    });
    if main_failed {
        return TrayState::Alert("\u{1f6a8} Deployment Failed on main".to_string());
    }

    TrayState::Neutral
}

fn load_embedded_icon(bytes: &[u8]) -> tray_icon::Icon {
    let image = image::load_from_memory(bytes)
        .expect("embedded icon is a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), width, height)
        .expect("icon RGBA dimensions are valid")
}

/// Opens a terminal window with `pulsos status --watch` on macOS.
#[cfg(target_os = "macos")]
pub fn launch_terminal_tui() {
    std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"Terminal\" to do script \"pulsos status --watch\"",
        ])
        .spawn()
        .ok();
}

/// Opens a CMD window with `pulsos status --watch` on Windows.
#[cfg(target_os = "windows")]
pub fn launch_terminal_tui() {
    std::process::Command::new("cmd")
        .args(["/C", "start", "cmd", "/K", "pulsos status --watch"])
        .spawn()
        .ok();
}

/// Menu-event receiver — thin wrapper so `commands/daemon.rs` doesn't need to
/// import muda directly.
pub fn menu_event_receiver() -> &'static muda::MenuEventReceiver {
    MenuEvent::receiver()
}
