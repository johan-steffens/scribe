// Rust guideline compliant 2026-02-21
//! Sync feature — provider trait, snapshot types, engine, and keychain.
//!
//! Gated behind the `sync` Cargo feature.
//!
//! The central abstraction is [`SyncProvider`], an async trait implemented by
//! concrete backends (e.g. GitHub Gist, a self-hosted HTTP server). Callers
//! exchange [`StateSnapshot`] values with the backend and handle failures
//! through [`SyncError`].

pub mod keychain;
pub mod providers;
pub mod snapshot;

use async_trait::async_trait;
use thiserror::Error;

#[doc(inline)]
pub use snapshot::StateSnapshot;

/// Errors that can occur during a sync operation.
#[derive(Debug, Error)]
pub enum SyncError {
    /// The remote resource does not exist yet (first push not done).
    #[error("not found: {0}")]
    NotFound(String),
    /// Authentication with the provider failed.
    #[error("auth error: {0}")]
    Auth(String),
    /// A network or I/O error occurred.
    #[error("transport error: {0}")]
    Transport(String),
    /// The remote snapshot cannot be deserialised.
    #[error("invalid snapshot: {0}")]
    InvalidSnapshot(String),
    /// The keychain is unavailable or the secret is missing.
    #[error("keychain error: {0}")]
    Keychain(String),
    /// Any other provider-specific error.
    #[error("{0}")]
    Other(String),
}

/// Abstraction over a remote sync backend.
///
/// Implementors must be `Send + Sync` so they can be shared across async tasks.
///
/// # Errors
///
/// Both methods return [`SyncError`] on failure. See variant docs for details.
#[async_trait]
pub trait SyncProvider: Send + Sync {
    /// Upload the local state snapshot to the remote.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] when the upload fails for any reason.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError>;

    /// Download the current remote snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::NotFound`] if no remote state exists yet, or
    /// another [`SyncError`] variant on transport or auth failures.
    async fn pull(&self) -> Result<StateSnapshot, SyncError>;
}
