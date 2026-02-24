//! Axum SSE server — streams `DaemonStateEvent` to connected TUI clients.

use std::future::IntoFuture;
use std::path::Path;
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Router,
};
use futures_util::StreamExt as _;
use rand::Rng;
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

/// Generates a 32-byte random token, hex-encodes it to a 64-char string,
/// writes it to `<config_dir>/daemon.token` with mode 0o600 (Unix), and
/// returns the hex string.
fn generate_and_persist_token(config_dir: &Path) -> anyhow::Result<String> {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(config_dir.join("daemon.token"))?
            .write_all(token.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(config_dir.join("daemon.token"), &token)?;

    Ok(token)
}

/// Axum middleware: validates `Authorization: Bearer <token>` on protected routes.
async fn bearer_auth(
    State(expected): State<Arc<String>>,
    request: Request,
    next: Next,
) -> Response {
    let ok = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|h| h == format!("Bearer {expected}"))
        .unwrap_or(false);
    if ok {
        next.run(request).await
    } else {
        axum::http::StatusCode::UNAUTHORIZED.into_response()
    }
}

fn build_router(tx: broadcast::Sender<DaemonStateEvent>, token: Arc<String>) -> Router {
    let protected = Router::new()
        .route("/api/stream", get(move || sse_handler(tx.clone())))
        .route_layer(middleware::from_fn_with_state(token, bearer_auth));

    Router::new()
        .merge(protected)
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
/// generate a bearer token at `~/.config/pulsos/daemon.token` (mode 0o600),
/// and spawn the server task. Returns the bound port number.
pub async fn start_server(tx: broadcast::Sender<DaemonStateEvent>) -> anyhow::Result<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let pulsos_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
        .join("pulsos");
    std::fs::create_dir_all(&pulsos_dir)?;

    // Persist the port so TUI clients can discover the daemon.
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

    // Generate bearer token and persist it for TUI clients to read.
    let token = Arc::new(generate_and_persist_token(&pulsos_dir)?);

    let app = build_router(tx, token);
    tokio::spawn(axum::serve(listener, app).into_future());
    Ok(port)
}
