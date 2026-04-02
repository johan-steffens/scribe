// Rust guideline compliant 2026-02-21
//! GitHub Gist sync provider.
//!
//! [`GistProvider`] stores the [`StateSnapshot`] as the content of a single
//! file (`scribe-state.json`) inside a private GitHub Gist. On the first push
//! a new Gist is created; subsequent pushes update the same Gist in place via
//! `PATCH`.
//!
//! ## Authentication
//!
//! A GitHub personal-access token must be stored in the OS keychain under the
//! service key `scribe.sync.gist.token` (see [`crate::sync::keychain`]). The
//! token needs the `gist` scope.  Run `scribe sync configure` to store it.
//!
//! # Examples
//!
//! ```no_run
//! use scribe::sync::providers::gist::GistProvider;
//! use scribe::sync::SyncProvider as _;
//!
//! # async fn example() -> Result<(), scribe::sync::SyncError> {
//! let mut provider = GistProvider::new(None)?;
//! // On first push the gist_id field is populated after the call returns.
//! # Ok(())
//! # }
//! ```

// DOCUMENTED-MAGIC: Provider unused until the sync engine is wired in a later task.
#![allow(dead_code, reason = "wired via engine in a later task")]

use async_trait::async_trait;
use serde_json::Value;

use crate::sync::keychain::KeychainStore;
use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── constants ──────────────────────────────────────────────────────────────

/// Base URL for the GitHub REST API v3.
///
/// All Gist endpoints are relative to this root. Changing this value would
/// redirect all API traffic to a different host.
const GITHUB_API_BASE: &str = "https://api.github.com";

/// Name of the file inside the Gist that stores the Scribe state snapshot.
///
/// The Gist contains exactly one file. Changing this name would make existing
/// Gists unreadable by future versions of the provider.
const GIST_FILE_NAME: &str = "scribe-state.json";

// ── GistProvider ───────────────────────────────────────────────────────────

/// Syncs state by storing a JSON snapshot in a private GitHub Gist.
///
/// On the first [`push`][GistProvider::push] a new private Gist is created and
/// `gist_id` is populated. Subsequent pushes update the same Gist via `PATCH`.
pub struct GistProvider {
    /// Existing Gist ID; `None` means not yet created.
    pub gist_id: Option<String>,
    /// Shared HTTP client — holds a connection pool internally.
    client: reqwest::Client,
}

impl std::fmt::Debug for GistProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GistProvider")
            .field("gist_id", &self.gist_id)
            .field("client", &"<reqwest::Client>")
            .finish()
    }
}

impl GistProvider {
    /// Creates a new `GistProvider`.
    ///
    /// `gist_id` is `None` on first use and populated after the initial push.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] if the HTTP client cannot be built
    /// (e.g. TLS initialisation fails).
    pub fn new(gist_id: Option<String>) -> Result<Self, SyncError> {
        let client = reqwest::Client::builder()
            .user_agent(super::USER_AGENT)
            .build()
            .map_err(|e| SyncError::Transport(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { gist_id, client })
    }
}

// ── SyncProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl SyncProvider for GistProvider {
    /// Serialises `snapshot` and writes it to the configured Gist.
    ///
    /// Creates a new Gist on the first call (updating `gist_id`). Subsequent
    /// calls patch the existing Gist.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the GitHub token is not in the keychain.
    /// - [`SyncError::Auth`] if the GitHub API returns HTTP 401.
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        let token = KeychainStore::get("gist", "token")?;
        let content = serde_json::to_string(snapshot)
            .map_err(|e| SyncError::Transport(format!("serialisation failed: {e}")))?;

        let (method, url, body) = if let Some(id) = &self.gist_id {
            let url = format!("{GITHUB_API_BASE}/gists/{id}");
            let body = serde_json::json!({
                "files": { GIST_FILE_NAME: { "content": content } }
            });
            (reqwest::Method::PATCH, url, body)
        } else {
            let url = format!("{GITHUB_API_BASE}/gists");
            let body = serde_json::json!({
                "description": "Scribe state sync",
                "public": false,
                "files": { GIST_FILE_NAME: { "content": content } }
            });
            (reqwest::Method::POST, url, body)
        };

        let response = self
            .client
            .request(method, &url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Auth(
                "GitHub API returned 401 — check your token with `scribe sync configure`"
                    .to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!(
                "GitHub API returned {status}"
            )));
        }

        Ok(())
    }

    /// Downloads the snapshot stored in the configured Gist.
    ///
    /// # Errors
    ///
    /// - [`SyncError::NotFound`] if `gist_id` is `None`.
    /// - [`SyncError::Keychain`] if the GitHub token is not in the keychain.
    /// - [`SyncError::Auth`] if the GitHub API returns HTTP 401.
    /// - [`SyncError::NotFound`] if the Gist no longer exists (HTTP 404).
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    /// - [`SyncError::InvalidSnapshot`] if the file content cannot be
    ///   deserialised as a [`StateSnapshot`].
    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        let id = self.gist_id.as_deref().ok_or_else(|| {
            SyncError::NotFound(
                "no gist_id configured — run `scribe sync configure` first".to_owned(),
            )
        })?;

        let token = KeychainStore::get("gist", "token")?;
        let url = format!("{GITHUB_API_BASE}/gists/{id}");

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(SyncError::NotFound(format!("gist '{id}' not found")));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::Auth(
                "GitHub API returned 401 — check your token with `scribe sync configure`"
                    .to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!(
                "GitHub API returned {status}"
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| SyncError::Transport(format!("failed to parse response: {e}")))?;

        let content = body["files"][GIST_FILE_NAME]["content"]
            .as_str()
            .ok_or_else(|| {
                SyncError::InvalidSnapshot(format!(
                    "gist response missing files.{GIST_FILE_NAME}.content"
                ))
            })?;

        serde_json::from_str(content).map_err(|e| {
            SyncError::InvalidSnapshot(format!("could not parse snapshot from gist: {e}"))
        })
    }
}
