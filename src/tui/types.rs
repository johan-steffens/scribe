// Rust guideline compliant 2026-02-21
//! Application state types for the Scribe TUI.
//!
//! This module contains the foundational enums and generic structs used by
//! [`super::App`]: [`View`], [`ViewState`], and [`InputMode`].

use crate::domain::{Project, Task};

// ── View enum ──────────────────────────────────────────────────────────────

/// All navigable views in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Home screen: today's tasks and active timer.
    Dashboard,
    /// Full project list.
    Projects,
    /// Full task list with filtering.
    Tasks,
    /// Todo list (Phase 4).
    Todos,
    /// Time tracking history (Phase 4).
    Tracker,
    /// Quick-capture inbox (Phase 4).
    Inbox,
    /// Reminder list (Phase 4).
    Reminders,
}

// ── ViewState ──────────────────────────────────────────────────────────────

/// Per-view list state: loaded items, cursor position, and live filter.
///
/// The `items` vector holds the full unfiltered dataset loaded from the
/// database. The `filter` string is applied at render time to produce the
/// visible subset. `selected` is an index into the **filtered** subset.
#[derive(Debug, Clone)]
pub struct ViewState<T> {
    /// All items loaded from the database (unfiltered).
    pub items: Vec<T>,
    /// Index into the filtered subset that is currently highlighted.
    pub selected: usize,
    /// Live filter string; empty means no filter is applied.
    pub filter: String,
}

impl<T> ViewState<T> {
    /// Creates an empty [`ViewState`] with no items and no filter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            filter: String::new(),
        }
    }
}

impl<T> Default for ViewState<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ── InputMode ─────────────────────────────────────────────────────────────

/// Tracks whether the user is currently typing a filter string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal navigation mode.
    Normal,
    /// Filter mode: keystrokes are appended to `ViewState::filter`.
    Filter,
}

// ── type aliases used across the TUI ──────────────────────────────────────

/// Convenience alias for the project list view state.
pub type ProjectViewState = ViewState<Project>;
/// Convenience alias for the task list view state.
pub type TaskViewState = ViewState<Task>;
