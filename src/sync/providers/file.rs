//! File-based sync provider — reads and writes state to a local JSON file.
//!
//! [`FileProvider`] is a simple provider that stores the [`StateSnapshot`] as
//! a pretty-printed JSON file at a given path. It is suitable for use with any
//! locally mounted filesystem, including network shares and cloud-synced
//! folders such as iCloud Drive (see [`crate::sync::providers::icloud`]).
//!
//! # Examples
//!
//! ```no_run
//! use std::path::PathBuf;
//! use scribe::sync::providers::file::FileProvider;
//! use scribe::sync::SyncProvider as _;
//!
//! # async fn example() -> Result<(), scribe::sync::SyncError> {
//! let provider = FileProvider::new(PathBuf::from("/tmp/scribe-state.json"));
//! let snapshot = provider.pull().await?;
//! # Ok(())
//! # }
//! ```

// DOCUMENTED-MAGIC: Provider unused until the sync engine is wired in a later task.

use std::path::PathBuf;

use async_trait::async_trait;

use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── FileProvider ───────────────────────────────────────────────────────────

/// Syncs state by reading and writing a JSON file at a local path.
///
/// The snapshot is stored as pretty-printed JSON. Parent directories are
/// created automatically on the first [`push`][FileProvider::push].
#[derive(Debug)]
pub struct FileProvider {
    path: PathBuf,
}

impl FileProvider {
    /// Creates a new `FileProvider` targeting `path`.
    ///
    /// The file does not need to exist yet; it is created on the first
    /// [`push`][FileProvider::push]. Parent directories are also created
    /// automatically.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

// ── SyncProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl SyncProvider for FileProvider {
    /// Serialises `snapshot` to JSON and writes it to the configured path.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] if the parent directory cannot be
    /// created, if serialisation fails, or if the file cannot be written.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SyncError::Transport(format!(
                    "failed to create parent directory {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let json = serde_json::to_string_pretty(snapshot)
            .map_err(|e| SyncError::Transport(format!("serialisation failed: {e}")))?;

        std::fs::write(&self.path, json).map_err(|e| {
            SyncError::Transport(format!("failed to write {}: {e}", self.path.display()))
        })?;

        Ok(())
    }

    /// Reads and deserialises the snapshot from the configured path.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::NotFound`] if the file does not exist,
    /// [`SyncError::Transport`] on I/O read errors, and
    /// [`SyncError::InvalidSnapshot`] if the file content cannot be
    /// deserialised as a [`StateSnapshot`].
    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        if !self.path.exists() {
            return Err(SyncError::NotFound(format!(
                "no state file at {}",
                self.path.display()
            )));
        }

        let content = std::fs::read_to_string(&self.path).map_err(|e| {
            SyncError::Transport(format!("failed to read {}: {e}", self.path.display()))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            SyncError::InvalidSnapshot(format!(
                "could not parse snapshot at {}: {e}",
                self.path.display()
            ))
        })
    }
}
