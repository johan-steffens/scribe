// Rust guideline compliant 2026-02-21
//! JSONBin.io sync provider.
//!
//! [`JsonBinProvider`] stores the [`StateSnapshot`] as the content of a private
//! JSONBin.io bin. On the first push a new bin is created; subsequent pushes
//! overwrite the bin in place via `PUT`.
//!
//! ## Authentication
//!
//! A JSONBin.io access key must be stored in the OS keychain under the service
//! key `scribe.sync.jsonbin.access_key` (see [`crate::sync::keychain`]). Run
//! `scribe sync configure` to store it.
//!
//! # Examples
//!
//! ```no_run
//! use scribe::sync::providers::jsonbin::JsonBinProvider;
//! use scribe::sync::SyncProvider as _;
//!
//! # async fn example() -> Result<(), scribe::sync::SyncError> {
//! let provider = JsonBinProvider::new(None)?;
//! let snapshot = provider.pull().await?;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use serde_json::Value;

use crate::sync::keychain::KeychainStore;
use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── constants ──────────────────────────────────────────────────────────────

/// Base URL for the JSONBin.io v3 API.
///
/// All bin endpoints are relative to this root. Changing this value would
/// redirect all API traffic to a different host.
const JSONBIN_API_BASE: &str = "https://api.jsonbin.io/v3";

// ── JsonBinProvider ────────────────────────────────────────────────────────

/// Syncs state by storing a JSON snapshot in a private JSONBin.io bin.
///
/// On the first [`push`][JsonBinProvider::push] a new private bin is created
/// and `bin_id` is populated. Subsequent pushes overwrite the bin via `PUT`.
pub struct JsonBinProvider {
    /// Bin ID; `None` until the first push creates the bin.
    pub bin_id: Option<String>,
    /// Shared HTTP client — holds a connection pool internally.
    client: reqwest::Client,
}

impl std::fmt::Debug for JsonBinProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonBinProvider")
            .field("bin_id", &self.bin_id)
            .field("client", &"<reqwest::Client>")
            .finish()
    }
}

impl JsonBinProvider {
    /// Creates a new `JsonBinProvider`.
    ///
    /// `bin_id` is `None` on first use and populated after the initial push.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] if the HTTP client cannot be built.
    pub fn new(bin_id: Option<String>) -> Result<Self, SyncError> {
        let client = reqwest::Client::builder()
            .user_agent("scribe-sync/1.0")
            .build()
            .map_err(|e| SyncError::Transport(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { bin_id, client })
    }
}

// ── SyncProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl SyncProvider for JsonBinProvider {
    /// Serialises `snapshot` and writes it to the configured JSONBin.io bin.
    ///
    /// Creates a new private bin on the first call (updating `bin_id`).
    /// Subsequent calls overwrite the existing bin.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the access key is not in the keychain.
    /// - [`SyncError::Transport`] on any non-success HTTP status or network
    ///   failure.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        let access_key = KeychainStore::get("jsonbin", "access_key")?;

        let (method, url) = if let Some(id) = &self.bin_id {
            let url = format!("{JSONBIN_API_BASE}/b/{id}");
            (reqwest::Method::PUT, url)
        } else {
            let url = format!("{JSONBIN_API_BASE}/b");
            (reqwest::Method::POST, url)
        };

        let mut request = self
            .client
            .request(method, &url)
            .header("X-Access-Key", &access_key)
            .json(snapshot);

        if self.bin_id.is_none() {
            request = request
                .header("X-Bin-Name", "scribe-state")
                .header("X-Bin-Private", "true");
        }

        let response = request
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            return Err(SyncError::Transport(format!(
                "JSONBin API returned {status}"
            )));
        }

        Ok(())
    }

    /// Downloads the snapshot stored in the configured JSONBin.io bin.
    ///
    /// # Errors
    ///
    /// - [`SyncError::NotFound`] if `bin_id` is `None`.
    /// - [`SyncError::Keychain`] if the access key is not in the keychain.
    /// - [`SyncError::NotFound`] if the bin no longer exists (HTTP 404).
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    /// - [`SyncError::InvalidSnapshot`] if the bin content cannot be
    ///   deserialised as a [`StateSnapshot`].
    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        let id = self.bin_id.as_deref().ok_or_else(|| {
            SyncError::NotFound(
                "no bin_id configured — run `scribe sync configure` first".to_owned(),
            )
        })?;

        let access_key = KeychainStore::get("jsonbin", "access_key")?;
        let url = format!("{JSONBIN_API_BASE}/b/{id}/latest");

        let response = self
            .client
            .get(&url)
            .header("X-Access-Key", &access_key)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(SyncError::NotFound(format!("JSONBin bin '{id}' not found")));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!(
                "JSONBin API returned {status}"
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| SyncError::Transport(format!("failed to parse response: {e}")))?;

        let record = body.get("record").ok_or_else(|| {
            SyncError::InvalidSnapshot("JSONBin response missing 'record' field".to_owned())
        })?;

        serde_json::from_value(record.clone()).map_err(|e| {
            SyncError::InvalidSnapshot(format!("could not parse snapshot from JSONBin: {e}"))
        })
    }
}
