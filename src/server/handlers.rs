// Rust guideline compliant 2026-02-21
//! `GET /state` and `PUT /state` handlers for the REST sync master server.
//!
//! Both handlers operate on [`ServerState`], a cheaply-cloneable wrapper
//! around a shared [`StateSnapshot`] protected by a `tokio::sync::RwLock`.
//!
//! - `GET /state` (`get_state`) acquires a read lock and serialises the
//!   current snapshot to JSON.
//! - `PUT /state` (`put_state`) acquires a write lock, merges the uploaded
//!   snapshot into local state, and returns the merged result.

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use tokio::sync::RwLock;

use crate::sync::{engine::SyncEngine, snapshot::StateSnapshot};

// ── ServerState ─────────────────────────────────────────────────────────────

/// Shared handler state, cheaply cloned per request by axum.
///
/// Holds a reference-counted read/write lock over the current snapshot and the
/// Bearer secret used by the authentication middleware. The secret is excluded
/// from the `Debug` representation to prevent leaking it into log output.
///
/// # Panics
///
/// Panics if the inner `RwLock` is poisoned, which cannot happen in normal
/// operation because all lock holders are async tasks that do not panic.
pub struct ServerState {
    /// The current local snapshot shared across all requests.
    pub(crate) snapshot: Arc<RwLock<StateSnapshot>>,
    /// The expected Bearer token used by the authentication middleware.
    pub(crate) secret: String,
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // DOCUMENTED-MAGIC: `secret` is intentionally omitted to prevent
        // token leakage through `tracing`/`Debug` output (M-PUBLIC-DEBUG).
        f.debug_struct("ServerState")
            .field("snapshot", &"<RwLock<StateSnapshot>>")
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        Self {
            snapshot: Arc::clone(&self.snapshot),
            secret: self.secret.clone(),
        }
    }
}

// ── handlers ─────────────────────────────────────────────────────────────────

/// Returns the current local snapshot as JSON.
///
/// Acquires a shared read lock on the snapshot. Multiple concurrent `GET`
/// requests can be served simultaneously.
pub async fn get_state(State(state): State<ServerState>) -> impl IntoResponse {
    let snap = state.snapshot.read().await.clone();
    Json(snap)
}

/// Merges the uploaded snapshot into local state and returns the merged result.
///
/// Acquires an exclusive write lock, applies
/// [`SyncEngine::merge_into`] with last-write-wins semantics, and serialises the
/// post-merge snapshot as the response body.
///
/// # Errors
///
/// Returns [`StatusCode::UNPROCESSABLE_ENTITY`] (422) if axum cannot
/// deserialise the request body as a [`StateSnapshot`] (handled automatically
/// by the [`Json`] extractor before this function is called).
pub async fn put_state(
    State(state): State<ServerState>,
    Json(remote): Json<StateSnapshot>,
) -> Result<impl IntoResponse, StatusCode> {
    let mut snap = state.snapshot.write().await;
    SyncEngine::merge_into(&mut snap, &remote);
    Ok(Json(snap.clone()))
}
