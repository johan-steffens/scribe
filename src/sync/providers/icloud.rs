// Rust guideline compliant 2026-02-21
//! iCloud Drive sync provider — delegates to [`FileProvider`] with tilde expansion.
//!
//! [`ICloudProvider`] wraps a [`FileProvider`] and expands a leading `~/` in
//! the configured path using the `$HOME` environment variable. This is safe
//! because `ICloudProvider` is macOS-only and `$HOME` is always set on macOS.
//!
//! # Examples
//!
//! ```no_run
//! use std::path::Path;
//! use scribe::sync::providers::icloud::ICloudProvider;
//! use scribe::sync::SyncProvider as _;
//!
//! # async fn example() -> Result<(), scribe::sync::SyncError> {
//! let provider = ICloudProvider::new(Path::new(
//!     "~/Library/Mobile Documents/com~apple~CloudDocs/Scribe/state.json",
//! ));
//! let snapshot = provider.pull().await?;
//! # Ok(())
//! # }
//! ```

// DOCUMENTED-MAGIC: Provider unused until the sync engine is wired in a later task.
#![allow(dead_code, reason = "wired via engine in a later task")]

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::file::FileProvider;
use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── tilde expansion ────────────────────────────────────────────────────────

/// Expands a leading `~/` in `path` to the value of `$HOME`.
///
/// If `path` does not start with `~/`, or if `$HOME` is not set, the path is
/// returned unchanged.
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path.to_owned()
}

// ── ICloudProvider ─────────────────────────────────────────────────────────

/// iCloud Drive sync provider; delegates file I/O to [`FileProvider`].
///
/// Tilde (`~/`) in the supplied path is expanded using `$HOME`. All push and
/// pull operations are forwarded to the inner [`FileProvider`].
#[derive(Debug)]
pub struct ICloudProvider {
    inner: FileProvider,
}

impl ICloudProvider {
    /// Creates an `ICloudProvider` with `path`, expanding a leading `~/`.
    ///
    /// `$HOME` is used for tilde expansion. If `$HOME` is not set the path is
    /// used as-is.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            inner: FileProvider::new(expand_tilde(path)),
        }
    }
}

// ── SyncProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl SyncProvider for ICloudProvider {
    /// Writes `snapshot` to the iCloud Drive file; delegates to [`FileProvider`].
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] on any I/O or serialisation failure.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        self.inner.push(snapshot).await
    }

    /// Reads the snapshot from the iCloud Drive file; delegates to [`FileProvider`].
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::NotFound`] if the file does not exist, or another
    /// [`SyncError`] variant on I/O or deserialisation failure.
    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        self.inner.pull().await
    }
}
