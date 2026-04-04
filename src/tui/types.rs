//! Application state types for the Scribe TUI.
//!
//! This module contains the foundational enums and generic structs used by
//! [`super::app::App`]: [`View`], [`ViewState`], [`InputMode`], and [`Modal`].
//!
//! # Modal system
//!
//! Phase 4 introduces a [`Modal`] enum that wraps a [`Form`], a
//! [`ConfirmDialog`], or a process-inbox dialog. At most one modal is active
//! at a time. The key handler in [`super::keys`] routes events to the active
//! modal first, then to the view.

use crate::domain::{CaptureItem, Project, Reminder, Task, TimeEntry, Todo};
use crate::tui::components::dialog::ConfirmDialog;
use crate::tui::components::form::Form;

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
    /// Todo list.
    Todos,
    /// Time tracking history.
    Tracker,
    /// Quick-capture inbox.
    Inbox,
    /// Reminder list.
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

// ── ConfirmContext ─────────────────────────────────────────────────────────

/// Identifies which action a confirmation dialog is associated with.
///
/// When the user confirms, the key handler reads this to know what mutation
/// to perform.
#[derive(Debug, Clone)]
pub enum ConfirmContext {
    /// Archive the todo identified by the given slug.
    ArchiveTodo(String),
    /// Archive the time entry identified by the given slug.
    ArchiveEntry(String),
    /// Delete the capture item identified by the given slug.
    DeleteCapture(String),
    /// Archive the reminder identified by the given slug.
    ArchiveReminder(String),
    /// Archive the project identified by the given slug.
    ArchiveProject(String),
    /// Archive the task identified by the given slug.
    ArchiveTask(String),
}

// ── FormContext ────────────────────────────────────────────────────────────

/// Identifies which form action is being performed.
///
/// The key handler reads this after submission to know which ops call to make.
#[derive(Debug, Clone)]
pub enum FormContext {
    /// Create a new todo.
    CreateTodo,
    /// Edit the todo with the given slug.
    EditTodo(String),
    /// Move the todo with the given slug to a new project.
    MoveTodo(String),
    /// Start a new timer (project slug is read from the form).
    StartTimer,
    /// Edit the note on the time entry with the given slug.
    EditEntryNote(String),
    /// Add a new capture item.
    CreateCapture,
    /// Create a new reminder.
    CreateReminder,
    /// Edit the reminder with the given slug.
    EditReminder(String),
    /// Create a new project.
    CreateProject,
    /// Edit the project with the given slug.
    EditProject(String),
    /// Create a new task.
    CreateTask,
    /// Edit the task with the given slug.
    EditTask(String),
    /// Process a capture item from the inbox.
    ///
    /// The inner string is the capture item slug.
    ProcessCapture(String),
}

// ── Modal ─────────────────────────────────────────────────────────────────

/// The active modal overlay, if any.
///
/// At most one modal is active at a time. The app state always holds exactly
/// one `Modal` value; `None` means no modal is visible.
#[derive(Debug)]
pub enum Modal {
    /// No modal is active.
    None,
    /// An inline create/edit form is shown.
    Form(Form, FormContext),
    /// A yes/no confirmation dialog is shown.
    Confirm(ConfirmDialog, ConfirmContext),
}

// ── type aliases used across the TUI ──────────────────────────────────────

/// Convenience alias for the project list view state.
pub type ProjectViewState = ViewState<Project>;
/// Convenience alias for the task list view state.
pub type TaskViewState = ViewState<Task>;
/// Convenience alias for the todo list view state.
pub type TodoViewState = ViewState<Todo>;
/// Convenience alias for the time-entry list view state.
pub type EntryViewState = ViewState<TimeEntry>;
/// Convenience alias for the capture-item list view state.
pub type CaptureViewState = ViewState<CaptureItem>;
/// Convenience alias for the reminder list view state.
pub type ReminderViewState = ViewState<Reminder>;
