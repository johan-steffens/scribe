// Rust guideline compliant 2026-02-21
//! Sync provider implementations.
//!
//! Each provider implements [`crate::sync::SyncProvider`]. The active provider
//! is constructed from [`crate::config::Config`] by the factory in this module
//! (added in a later task).

pub mod file;
#[cfg(target_os = "macos")]
pub mod icloud;
