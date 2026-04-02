// Rust guideline compliant 2026-02-21
//! Dropbox API v2 sync provider.
//!
//! [`DropboxProvider`] stores the [`StateSnapshot`] as a file at a configurable
//! path inside the user's Dropbox. Pushes overwrite the file unconditionally;
//! pulls retrieve the current version.
//!
//! ## Authentication
//!
//! A Dropbox `OAuth2` access token must be stored in the OS keychain under the
//! service key `scribe.sync.dropbox.access_token` (see
//! [`crate::sync::keychain`]). Run `scribe sync configure` to store it.
//!
//! # Examples
//!
//! ```no_run
//! use scribe::sync::providers::dropbox::DropboxProvider;
//! use scribe::sync::SyncProvider as _;
//!
//! # async fn example() -> Result<(), scribe::sync::SyncError> {
//! let provider = DropboxProvider::new("/scribe-state.json")?;
//! let snapshot = provider.pull().await?;
//! # Ok(())
//! # }
//! ```

// DOCUMENTED-MAGIC: Provider unused until the sync engine is wired in a later task.

use async_trait::async_trait;

use crate::sync::keychain::KeychainStore;
use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── constants ──────────────────────────────────────────────────────────────

/// Base URL for Dropbox Content API v2 (file upload/download endpoints).
///
/// File metadata endpoints use `api.dropboxapi.com`; content endpoints use
/// this separate host. Changing this value would break all upload and download
/// operations.
const DROPBOX_CONTENT_API: &str = "https://content.dropboxapi.com/2";

// ── DropboxProvider ────────────────────────────────────────────────────────

/// Syncs state by reading and writing a file in the user's Dropbox.
///
/// The snapshot is stored as JSON at the path supplied to
/// [`DropboxProvider::new`]. Pushes overwrite the file unconditionally using
/// `mode: "overwrite"`.
pub struct DropboxProvider {
    /// Remote Dropbox path, e.g. `"/scribe-state.json"`.
    path: String,
    /// Shared HTTP client — holds a connection pool internally.
    client: reqwest::Client,
}

impl std::fmt::Debug for DropboxProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DropboxProvider")
            .field("path", &self.path)
            .field("client", &"<reqwest::Client>")
            .finish()
    }
}

impl DropboxProvider {
    /// Creates a new `DropboxProvider` targeting `path` in Dropbox.
    ///
    /// `path` must be an absolute Dropbox path, e.g. `"/scribe-state.json"`.
    /// Both `&str` and `String` are accepted.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] if the HTTP client cannot be built.
    pub fn new(path: impl Into<String>) -> Result<Self, SyncError> {
        let client = reqwest::Client::builder()
            .user_agent(super::USER_AGENT)
            .build()
            .map_err(|e| SyncError::Transport(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            path: path.into(),
            client,
        })
    }
}

// ── SyncProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl SyncProvider for DropboxProvider {
    /// Serialises `snapshot` and uploads it to the configured Dropbox path.
    ///
    /// Always overwrites the existing file (`mode: "overwrite"`). The file is
    /// created if it does not yet exist.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the access token is not in the keychain.
    /// - [`SyncError::Auth`] if Dropbox returns HTTP 401.
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        let token = KeychainStore::get("dropbox", "access_token")?;
        let body = serde_json::to_vec(snapshot)
            .map_err(|e| SyncError::Transport(format!("serialisation failed: {e}")))?;

        // The Dropbox-API-Arg header encodes the upload parameters as JSON.
        // `autorename: false` means a conflict will overwrite; `mute: true`
        // suppresses Dropbox desktop notifications for programmatic writes.
        let api_arg = serde_json::json!({
            "path": self.path,
            "mode": "overwrite",
            "autorename": false,
            "mute": true
        })
        .to_string();

        let url = format!("{DROPBOX_CONTENT_API}/files/upload");
        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .header("Dropbox-API-Arg", api_arg)
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Auth(
                "Dropbox returned 401 — check your token with `scribe sync configure`".to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!("Dropbox returned {status}")));
        }

        Ok(())
    }

    /// Downloads the snapshot from the configured Dropbox path.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the access token is not in the keychain.
    /// - [`SyncError::Auth`] if Dropbox returns HTTP 401.
    /// - [`SyncError::NotFound`] if Dropbox returns HTTP 409 (path not found).
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    /// - [`SyncError::InvalidSnapshot`] if the file content cannot be
    ///   deserialised as a [`StateSnapshot`].
    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        let token = KeychainStore::get("dropbox", "access_token")?;

        // For downloads the path is also passed via Dropbox-API-Arg.
        let api_arg = serde_json::json!({ "path": self.path }).to_string();

        let url = format!("{DROPBOX_CONTENT_API}/files/download");
        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .header("Dropbox-API-Arg", api_arg)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        // Dropbox uses 409 Conflict when the path does not exist.
        if status == reqwest::StatusCode::CONFLICT {
            return Err(SyncError::NotFound(format!(
                "Dropbox path '{}' not found",
                self.path
            )));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Auth(
                "Dropbox returned 401 — check your token with `scribe sync configure`".to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!("Dropbox returned {status}")));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| SyncError::Transport(format!("failed to read response body: {e}")))?;

        serde_json::from_slice(&bytes).map_err(|e| {
            SyncError::InvalidSnapshot(format!("could not parse snapshot from Dropbox: {e}"))
        })
    }
}
