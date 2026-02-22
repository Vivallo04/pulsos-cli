//! Axum SSE server — streams `DaemonStateEvent` to connected TUI clients.

use std::future::IntoFuture;

use axum::{
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::tui::app::DataSnapshot;

/// The event type broadcast over the SSE stream.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DaemonStateEvent {
    pub snapshot: DataSnapshot,
    /// Monotonic sequence number — increments on every snapshot change.
    pub seq: u64,
    /// Daemon binary version string.
    pub daemon_version: String,
}

fn build_router(tx: broadcast::Sender<DaemonStateEvent>) -> Router {
    Router::new()
        .route("/api/stream", get(move || sse_handler(tx.clone())))
        .route("/health", get(|| async { "ok" }))
}

async fn sse_handler(
    tx: broadcast::Sender<DaemonStateEvent>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, axum::Error>>> {
    let rx = tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|r| {
        let item = r.ok().and_then(|ev| {
            serde_json::to_string(&ev)
                .ok()
                .map(|json| Ok(Event::default().data(json)))
        });
        async move { item }
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// Bind to an ephemeral port, write the port to `~/.config/pulsos/daemon.port`,
/// and spawn the server task. Returns the bound port number.
pub async fn start_server(tx: broadcast::Sender<DaemonStateEvent>) -> anyhow::Result<u16> {
    let app = build_router(tx);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    // Persist the port so TUI clients can discover the daemon.
    if let Some(config_dir) = dirs::config_dir() {
        let pulsos_dir = config_dir.join("pulsos");
        std::fs::create_dir_all(&pulsos_dir)?;
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;

            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(pulsos_dir.join("daemon.port"))?
                .write_all(port.to_string().as_bytes())?;
        }
        #[cfg(not(unix))]
        std::fs::write(pulsos_dir.join("daemon.port"), port.to_string())?;
    }

    tokio::spawn(axum::serve(listener, app).into_future());
    Ok(port)
}
