// Rust guideline compliant 2026-02-21
//! Sync provider implementations.
//!
//! Each provider implements [`crate::sync::SyncProvider`]. The active provider
//! is constructed from [`crate::config::Config`] by the factory in this module
//! (added in a later task).

pub mod dropbox;
pub mod file;
pub mod gist;
#[cfg(target_os = "macos")]
pub mod icloud;
pub mod jsonbin;
pub mod rest;
pub mod s3;

// ── shared constants ───────────────────────────────────────────────────────

/// HTTP `User-Agent` header sent by all sync providers.
///
/// Identifies Scribe to remote APIs for rate-limiting and logging purposes.
/// Update the version suffix when the sync protocol changes in a
/// breaking way. Format: `<application>/<version>`.
pub(crate) const USER_AGENT: &str = "scribe-sync/1.0";
