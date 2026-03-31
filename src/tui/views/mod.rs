// Rust guideline compliant 2026-02-21
//! TUI view modules — one per navigable screen.
//!
//! Each view is a pure rendering function that accepts `&App` and writes to a
//! [`ratatui::Frame`]. No view holds mutable state.
//!
//! # Phase 3 views
//!
//! | Module | View |
//! |---|---|
//! | [`dashboard`] | Home: today's tasks + active timer |
//! | [`projects`] | Full project list with live filter |
//! | [`tasks`] | Full task list with live filter |
//!
//! # Phase 4 placeholders
//!
//! [`placeholder`] renders a centred "Coming in Phase 4" message for the
//! Todos, Tracker, Inbox, and Reminders views.

pub mod dashboard;
pub mod placeholder;
pub mod projects;
pub mod tasks;
