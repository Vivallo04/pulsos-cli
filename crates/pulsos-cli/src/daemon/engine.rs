//! Background engine — wraps the existing `run_poller` and broadcasts each
//! snapshot update to SSE subscribers.

use tokio::sync::{broadcast, mpsc, watch};

use pulsos_core::config::types::PulsosConfig;

use crate::daemon::server::DaemonStateEvent;
use crate::tui::app::DataSnapshot;
use crate::tui::poll::{run_poller, PollerCommand};

pub async fn run_engine(config: PulsosConfig, broadcast_tx: broadcast::Sender<DaemonStateEvent>) {
    let (watch_tx, mut watch_rx) = watch::channel(DataSnapshot::default());
    // Daemon doesn't issue poller commands — use a dummy channel that's never sent to.
    let (_cmd_tx, cmd_rx): (mpsc::Sender<PollerCommand>, _) = mpsc::channel(8);

    tokio::spawn(run_poller(config, watch_tx, cmd_rx));

    let mut seq: u64 = 0;
    loop {
        if watch_rx.changed().await.is_err() {
            // Poller task exited.
            break;
        }
        let snapshot = watch_rx.borrow().clone();
        seq += 1;
        let event = DaemonStateEvent {
            snapshot,
            seq,
            daemon_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        // Ignore send errors — it's fine if no subscribers have connected yet.
        let _ = broadcast_tx.send(event);
    }
}
