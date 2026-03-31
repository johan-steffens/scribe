// Rust guideline compliant 2026-02-21
//! Reusable TUI widget components.
//!
//! Components are pure rendering functions that accept `&App` or direct data
//! parameters and render into a [`ratatui::Frame`]. They hold no mutable state.
//!
//! # Modules
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`status_bar`] | Two-line bottom status bar (timer + hints/errors) |
//! | [`table`] | Stateful highlighted table widget |

pub mod status_bar;
pub mod table;
