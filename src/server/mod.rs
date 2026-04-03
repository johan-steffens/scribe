// Rust guideline compliant 2026-02-21
//! REST sync master server.
//!
//! Started by the daemon when `sync.provider = "rest"` and
//! `sync.rest.role = "master"`.  Exposes `GET /state` and `PUT /state`,
//! both protected by a Bearer token read from the OS keychain.
//!
//! Pass `port = 0` to let the OS assign an available ephemeral port (useful
//! in tests).
//!
//! # Examples
//!
//! ```no_run
//! use scribe::server::start_server;
//! use scribe::sync::snapshot::StateSnapshot;
//! use chrono::Utc;
//! use uuid::Uuid;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let snap = StateSnapshot {
//!     snapshot_at: Utc::now(),
//!     machine_id: Uuid::nil(),
//!     schema_version: StateSnapshot::SCHEMA_VERSION,
//!     projects: vec![],
//!     tasks: vec![],
//!     todos: vec![],
//!     time_entries: vec![],
//!     reminders: vec![],
//!     capture_items: vec![],
//! };
//! let (port, _handle) = start_server(0, "my-secret".to_owned(), snap, Default::default()).await;
//! println!("listening on port {port}");
//! # }
//! ```
//!
//! # Scope
//!
//! The REST server module is gated by `sync` and is only built when the
//! `sync` feature is enabled.

// DOCUMENTED-MAGIC: The server module is fully implemented but not yet called
// from main.rs dispatch. Dead code warnings are expected until the daemon wires
// this in a future task.
#![allow(
    dead_code,
    reason = "server items wired via daemon in a later task; all items are implemented and tested"
)]

pub mod auth;
pub mod handlers;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{Router, middleware, routing::get, routing::put};
use tokio::{net::TcpListener, sync::RwLock, task::JoinHandle};
use uuid::Uuid;

use crate::sync::snapshot::StateSnapshot;
use handlers::ServerState;

/// Bind address for the REST sync server.
///
/// DOCUMENTED-MAGIC: Binding to `0.0.0.0` exposes the server on all network
/// interfaces, which is required for a local-network sync master that must be
/// reachable from other devices. The port is configurable at call-site.
const BIND_HOST: &str = "0.0.0.0";

const DEFAULT_STATE_REFRESH_INTERVAL_SECS: u64 = 60;

/// Configuration for the REST sync master server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Path to the database file.
    pub db_path: PathBuf,
    /// Machine ID used when reading from the database.
    pub machine_id: Uuid,
    /// Interval in seconds between refreshing state from the database.
    pub refresh_interval_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::new(),
            machine_id: Uuid::nil(),
            refresh_interval_secs: DEFAULT_STATE_REFRESH_INTERVAL_SECS,
        }
    }
}

/// Starts the REST sync master server on the given port.
///
/// Binds a `TcpListener` on `0.0.0.0:<port>` (pass `0` for an OS-assigned
/// ephemeral port), wires up axum routes with Bearer-token authentication, and
/// spawns the server as a background task. A background task also periodically
/// refreshes the server's state from the database to pick up local changes.
///
/// Returns the bound port (useful when `port = 0`) and a [`JoinHandle`] for
/// the server task. The server runs until the handle is dropped or aborted.
///
/// # Panics
///
/// Panics on startup if the `TcpListener` cannot bind the requested port.
/// This is intentional: if the daemon cannot acquire its configured port there
/// is no point continuing. Fix the port conflict and restart.
pub async fn start_server(
    port: u16,
    secret: String,
    initial_snapshot: StateSnapshot,
    config: ServerConfig,
) -> (u16, JoinHandle<()>) {
    let state = ServerState {
        snapshot: Arc::new(RwLock::new(initial_snapshot)),
        secret: secret.clone(),
        db_path: config.db_path.clone(),
        machine_id: config.machine_id,
    };

    let app = Router::new()
        .route("/state", get(handlers::get_state))
        .route("/state", put(handlers::put_state))
        .layer(middleware::from_fn({
            let secret_for_mw = secret.clone();
            move |headers, mut req: axum::extract::Request, next: middleware::Next| {
                req.extensions_mut().insert(secret_for_mw.clone());
                auth::require_bearer(headers, req, next)
            }
        }))
        .with_state(state.clone());

    // Spawn background task to periodically refresh state from database.
    let refresh_state = state.clone();
    let refresh_interval = Duration::from_secs(config.refresh_interval_secs);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(refresh_interval);
        loop {
            interval.tick().await;
            refresh_state.refresh_from_db().await;
        }
    });

    // DOCUMENTED-MAGIC: panic here is intentional — if the daemon cannot bind
    // its configured port there is no point continuing. The operator must
    // resolve the port conflict before restarting (M-PANIC-ON-BUG).
    let listener = TcpListener::bind(format!("{BIND_HOST}:{port}"))
        .await
        .expect("daemon failed to bind REST sync port");

    let bound_port = listener
        .local_addr()
        .expect("listener has no local address")
        .port();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("REST sync server error");
    });

    (bound_port, handle)
}
