// Rust guideline compliant 2026-02-21
//! Integration tests for the TUI subsystem.
//!
//! Uses `ratatui`'s `TestBackend` to render views into a virtual terminal
//! buffer and assert on the visual output.

pub mod tui;
