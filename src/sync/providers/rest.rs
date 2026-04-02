// Rust guideline compliant 2026-02-21
//! Self-hosted REST sync provider (client side).
//!
//! [`RestProvider`] connects to a self-hosted Scribe sync server and exchanges
//! [`StateSnapshot`] values over a simple HTTP API. The server exposes two
//! endpoints: `PUT /state` to upload and `GET /state` to download.
//!
//! ## Authentication
//!
//! A shared secret must be stored in the OS keychain under the service key
//! `scribe.sync.rest.secret` (see [`crate::sync::keychain`]). It is sent as a
//! Bearer token in the `Authorization` header. Run `scribe sync configure` to
//! store it.
//!
//! # Examples
//!
//! ```no_run
//! use scribe::sync::providers::rest::RestProvider;
//! use scribe::sync::SyncProvider as _;
//!
//! # async fn example() -> Result<(), scribe::sync::SyncError> {
//! let provider = RestProvider::new("http://192.168.1.10:7171")?;
//! let snapshot = provider.pull().await?;
//! # Ok(())
//! # }
//! ```

// DOCUMENTED-MAGIC: Provider unused until the sync engine is wired in a later task.
#![allow(dead_code, reason = "wired via engine in a later task")]

use async_trait::async_trait;

use crate::sync::keychain::KeychainStore;
use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── RestProvider ───────────────────────────────────────────────────────────

/// Syncs state with a self-hosted REST server at a configurable base URL.
///
/// Pushes serialise the snapshot to JSON and `PUT` it to `{url}/state`.
/// Pulls fetch the snapshot with `GET {url}/state`.
pub struct RestProvider {
    /// Base URL of the sync server, e.g. `"http://192.168.1.10:7171"`.
    url: String,
    /// Shared HTTP client — holds a connection pool internally.
    client: reqwest::Client,
}

impl std::fmt::Debug for RestProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestProvider")
            .field("url", &self.url)
            .field("client", &"<reqwest::Client>")
            .finish()
    }
}

impl RestProvider {
    /// Creates a new `RestProvider` targeting `url`.
    ///
    /// `url` is the base URL of the sync server. Trailing slashes are
    /// acceptable; the provider appends `/state` for all requests.
    /// Both `&str` and `String` are accepted.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] if the HTTP client cannot be built.
    pub fn new(url: impl Into<String>) -> Result<Self, SyncError> {
        let client = reqwest::Client::builder()
            .user_agent(super::USER_AGENT)
            .build()
            .map_err(|e| SyncError::Transport(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            url: url.into(),
            client,
        })
    }
}

// ── SyncProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl SyncProvider for RestProvider {
    /// Serialises `snapshot` and `PUT`s it to `{url}/state`.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the shared secret is not in the keychain.
    /// - [`SyncError::Auth`] if the server returns HTTP 401.
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        let secret = KeychainStore::get("rest", "secret")?;
        let url = format!("{}/state", self.url);

        let response = self
            .client
            .put(&url)
            .bearer_auth(&secret)
            .json(snapshot)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Auth(
                "REST server returned 401 — check your secret with `scribe sync configure`"
                    .to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!(
                "REST server returned {status}"
            )));
        }

        Ok(())
    }

    /// Downloads the snapshot from `{url}/state`.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the shared secret is not in the keychain.
    /// - [`SyncError::NotFound`] if the server returns HTTP 404.
    /// - [`SyncError::Auth`] if the server returns HTTP 401.
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    /// - [`SyncError::InvalidSnapshot`] if the response body cannot be
    ///   deserialised as a [`StateSnapshot`].
    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        let secret = KeychainStore::get("rest", "secret")?;
        let url = format!("{}/state", self.url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&secret)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(SyncError::NotFound(
                "REST server has no state yet — run a push first".to_owned(),
            ));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Auth(
                "REST server returned 401 — check your secret with `scribe sync configure`"
                    .to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!(
                "REST server returned {status}"
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SyncError::InvalidSnapshot(format!("could not parse snapshot: {e}")))
    }
}
