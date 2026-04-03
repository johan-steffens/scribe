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
    /// The gist_id is persisted to disk when a new Gist is created, so
    /// subsequent runs will update the same Gist rather than creating new ones.
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

        // If no gist_id provided by config, check the persisted file
        let resolved_gist_id = if gist_id.is_some() {
            gist_id.clone()
        } else {
            Self::load_persisted_gist_id()
        };
        
        match (&gist_id, &resolved_gist_id) {
            (Some(id), _) => tracing::debug!(gist_id = %id, "GistProvider created with config gist_id"),
            (None, Some(id)) => tracing::debug!(gist_id = %id, "GistProvider created with persisted gist_id from file"),
            (None, None) => tracing::debug!("GistProvider created with no gist_id — will create new gist on first push"),
        }

        Ok(Self { gist_id: resolved_gist_id, client })
    }

    /// Returns the path where we persist the gist_id across restarts.
    fn gist_id_path() -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("", "", "scribe")
            .map(|d| d.data_local_dir().join("gist-id"))
    }

    /// Loads the persisted gist_id from disk, if one exists.
    fn load_persisted_gist_id() -> Option<String> {
        let path = Self::gist_id_path()?;
        let id = std::fs::read_to_string(&path).ok()?.trim().to_owned().into();
        tracing::debug!(gist_id_path = %path.display(), gist_id = %id, "Loaded persisted gist_id from file");
        Some(id)
    }

    /// Persists a newly-created gist ID to disk and updates config.
    fn persist_new_gist_id(gist_id: &str) {
        if let Some(path) = Self::gist_id_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(&path, gist_id).is_ok() {
                tracing::info!(gist_id, path = %path.display(), "Persisted new gist_id to file");
            } else {
                tracing::error!(gist_id, path = %path.display(), "Failed to persist gist_id to file");
            }
        }

        if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "scribe") {
            let config_path = proj_dirs.data_local_dir().join("config.toml");
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(mut toml_content) = content.parse::<toml::Table>() {
                    if let Some(sync) = toml_content.get_mut("sync").and_then(|s| s.as_table_mut()) {
                        if let Some(gist) = sync.get_mut("gist").and_then(|g| g.as_table_mut()) {
                            gist.insert("gist_id".to_string(), toml::Value::String(gist_id.to_string()));
                            if let Ok(new_content) = toml::to_string(&toml_content) {
                                if std::fs::write(&config_path, new_content).is_ok() {
                                    tracing::info!(config_path = %config_path.display(), gist_id, "Updated gist_id in config.toml");
                                } else {
                                    tracing::error!(config_path = %config_path.display(), "Failed to write gist_id to config.toml");
                                }
                            }
                        }
                    }
                }
            } else {
                tracing::debug!(config_path = %config_path.display(), "No existing config.toml to update gist_id");
            }
        }
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
        let token = match KeychainStore::get("gist", "token") {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "gist.push: failed to get token from keychain");
                return Err(e);
            }
        };
        tracing::debug!("gist.push: got token from keychain");
        let content = serde_json::to_string(snapshot)
            .map_err(|e| SyncError::Transport(format!("serialisation failed: {e}")))?;

        let (method, url, body, is_new_gist) = if let Some(id) = &self.gist_id {
            let url = format!("{GITHUB_API_BASE}/gists/{id}");
            let body = serde_json::json!({
                "files": { GIST_FILE_NAME: { "content": content } }
            });
            tracing::info!(gist_id = %id, "gist.push: updating existing gist via PATCH");
            (reqwest::Method::PATCH, url, body, false)
        } else {
            let url = format!("{GITHUB_API_BASE}/gists");
            let body = serde_json::json!({
                "description": "Scribe state sync",
                "public": false,
                "files": { GIST_FILE_NAME: { "content": content } }
            });
            tracing::info!("gist.push: no gist_id set — creating new gist via POST");
            (reqwest::Method::POST, url, body, true)
        };

        let response = self
            .client
            .request(method, &url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "HTTP request failed");
                SyncError::Transport(format!("request failed: {e}"))
            })?;

        let status = response.status();
        tracing::debug!(status = %status, "GitHub API response status");

        if status == reqwest::StatusCode::UNAUTHORIZED {
            tracing::error!("GitHub API returned 401 Unauthorized");
            return Err(SyncError::Auth(
                "GitHub API returned 401 — check your token with `scribe sync configure`"
                    .to_owned(),
            ));
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!(status = %status, error = %error_text, "GitHub API error");
            return Err(SyncError::Transport(format!(
                "GitHub API returned {status}: {error_text}"
            )));
        }

        // If this was a new gist, extract and persist the gist_id from the response.
        if is_new_gist {
            if let Ok(body) = response.json::<Value>().await {
                if let Some(id) = body.get("id").and_then(|v| v.as_str()) {
                    tracing::info!(gist_id = %id, "gist.push: new gist created, persisting ID");
                    Self::persist_new_gist_id(id);
                } else {
                    tracing::warn!("gist.push: new gist created but no id in response: {:?}", body);
                }
            } else {
                tracing::error!("gist.push: failed to parse new gist response");
            }
        } else {
            tracing::info!(gist_id = ?self.gist_id, "gist.push: existing gist updated");
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
            tracing::error!("gist.pull: no gist_id configured");
            SyncError::NotFound(
                "no gist_id configured — run `scribe sync configure` first".to_owned(),
            )
        })?;

        let token = match KeychainStore::get("gist", "token") {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "gist.pull: failed to get token from keychain");
                return Err(e);
            }
        };
        let url = format!("{GITHUB_API_BASE}/gists/{id}");

        tracing::info!(gist_id = %id, "gist.pull: fetching gist");

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "gist.pull: HTTP request failed");
                SyncError::Transport(format!("request failed: {e}"))
            })?;

        let status = response.status();
        tracing::info!(status = %status, "gist.pull: response status");

        if status == reqwest::StatusCode::NOT_FOUND {
            tracing::error!(gist_id = %id, "gist.pull: gist not found (404)");
            return Err(SyncError::NotFound(format!("gist '{id}' not found")));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            tracing::error!("gist.pull: GitHub API returned 401 Unauthorized");
            return Err(SyncError::Auth(
                "GitHub API returned 401 — check your token with `scribe sync configure`"
                    .to_owned(),
            ));
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!(status = %status, error = %error_text, "gist.pull: GitHub API error");
            return Err(SyncError::Transport(format!(
                "GitHub API returned {status}: {error_text}"
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "gist.pull: failed to parse JSON response");
                SyncError::Transport(format!("failed to parse response: {e}"))
            })?;

        let content = body["files"][GIST_FILE_NAME]["content"]
            .as_str()
            .ok_or_else(|| {
                tracing::error!(missing_file = %GIST_FILE_NAME, "gist.pull: scribe-state.json not found in gist");
                SyncError::InvalidSnapshot(format!(
                    "gist response missing files.{GIST_FILE_NAME}.content"
                ))
            })?;

        tracing::info!(content_len = content.len(), "gist.pull: parsed snapshot from gist");

        serde_json::from_str(content).map_err(|e| {
            tracing::error!(error = %e, "gist.pull: failed to parse snapshot JSON");
            SyncError::InvalidSnapshot(format!("could not parse snapshot from gist: {e}"))
        })
    }
}
