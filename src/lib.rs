// Rust guideline compliant 2026-02-21
//! Scribe library — exposes domain types and feature-gated modules for testing.
//!
//! The primary entry point is `src/main.rs`. This library target exists to
//! make integration tests and downstream tooling able to import domain types
//! and feature-gated modules directly without going through the binary.

pub mod config;
pub mod domain;
#[cfg(feature = "sync")]
pub mod sync;
