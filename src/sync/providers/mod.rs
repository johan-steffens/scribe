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
