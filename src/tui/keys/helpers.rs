//! Selection, navigation, and utility helpers for the key handler.
//!
//! These are pure helper functions that read or compute values from `App`
//! state without performing any mutations or I/O.

use chrono::{NaiveDateTime, TimeZone};

use crate::tui::app::App;
use crate::tui::types::{Modal, View};

// ── view switching ─────────────────────────────────────────────────────────

/// Switches to `view`, resetting mode, help, and filter.
pub(super) fn switch_view(app: &mut App, view: View) {
    app.active_view = view;
    app.input_mode = crate::tui::types::InputMode::Normal;
    app.show_help = false;
    app.modal = Modal::None;
    current_filter_mut(app).clear();
}

// ── cursor movement ────────────────────────────────────────────────────────

/// Moves the selection cursor down by one in the active list view.
pub(super) fn move_selection_down(app: &mut App) {
    let len = app.filtered_len();
    if len == 0 {
        return;
    }
    let sel = app.selected_mut();
    if *sel + 1 < len {
        *sel += 1;
    }
}

/// Moves the selection cursor up by one in the active list view.
pub(super) fn move_selection_up(app: &mut App) {
    let sel = app.selected_mut();
    if *sel > 0 {
        *sel -= 1;
    }
}

// ── selection accessors ────────────────────────────────────────────────────

/// Returns the currently selected visible todo, if any.
pub(super) fn selected_todo(app: &App) -> Option<&crate::domain::Todo> {
    let filter = app.todos.filter.to_lowercase();
    let visible: Vec<_> = app
        .todos
        .items
        .iter()
        .filter(|t| filter.is_empty() || t.title.to_lowercase().contains(&filter))
        .collect();
    visible.get(app.todos.selected).copied()
}

/// Returns the currently selected visible time entry, if any.
pub(super) fn selected_entry(app: &App) -> Option<&crate::domain::TimeEntry> {
    app.entries.items.get(app.entries.selected)
}

/// Returns the currently selected visible capture item, if any.
pub(super) fn selected_capture(app: &App) -> Option<&crate::domain::CaptureItem> {
    let filter = app.captures.filter.to_lowercase();
    let visible: Vec<_> = app
        .captures
        .items
        .iter()
        .filter(|c| filter.is_empty() || c.body.to_lowercase().contains(&filter))
        .collect();
    visible.get(app.captures.selected).copied()
}

/// Returns the currently selected visible reminder, if any.
pub(super) fn selected_reminder(app: &App) -> Option<&crate::domain::Reminder> {
    let filter = app.reminders.filter.to_lowercase();
    let visible: Vec<_> = app
        .reminders
        .items
        .iter()
        .filter(|r| {
            filter.is_empty()
                || r.message
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&filter)
        })
        .collect();
    visible.get(app.reminders.selected).copied()
}

/// Returns the currently selected visible project, if any.
pub(super) fn selected_project(app: &App) -> Option<&crate::domain::Project> {
    let filter = app.projects.filter.to_lowercase();
    let visible: Vec<_> = app
        .projects
        .items
        .iter()
        .filter(|p| {
            filter.is_empty()
                || p.slug.to_lowercase().contains(&filter)
                || p.name.to_lowercase().contains(&filter)
        })
        .collect();
    visible.get(app.projects.selected).copied()
}

/// Returns the currently selected visible task, if any.
pub(super) fn selected_task(app: &App) -> Option<&crate::domain::Task> {
    let filter = app.tasks.filter.to_lowercase();
    let visible: Vec<_> = app
        .tasks
        .items
        .iter()
        .filter(|t| filter.is_empty() || t.title.to_lowercase().contains(&filter))
        .collect();
    visible.get(app.tasks.selected).copied()
}

// ── misc utilities ─────────────────────────────────────────────────────────

/// Returns a mutable reference to the filter string for the active view.
pub(super) fn current_filter_mut(app: &mut App) -> &mut String {
    match app.active_view {
        View::Projects => &mut app.projects.filter,
        View::Tasks | View::Dashboard => &mut app.tasks.filter,
        View::Todos => &mut app.todos.filter,
        View::Tracker => &mut app.entries.filter,
        View::Inbox => &mut app.captures.filter,
        View::Reminders => &mut app.reminders.filter,
    }
}

/// Collects the slugs of all non-archived projects for select fields.
pub(super) fn project_slugs(app: &App) -> Vec<String> {
    app.projects
        .items
        .iter()
        .filter(|p| p.archived_at.is_none())
        .map(|p| p.slug.clone())
        .collect()
}

/// Parses a datetime string in `YYYY-MM-DD HH:MM` or RFC 3339 format.
///
/// # Errors
///
/// Returns an error if the string cannot be parsed as a valid datetime.
pub(super) fn parse_datetime(s: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    // Try RFC 3339 first.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }

    // Try "YYYY-MM-DD HH:MM" or "YYYY-MM-DD HH:MM:SS".
    let normalized = s.replace(' ', "T");
    let normalized = if normalized.len() == 16 {
        format!("{normalized}:00")
    } else {
        normalized
    };

    NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S")
        .map(|ndt| chrono::Utc.from_utc_datetime(&ndt))
        .map_err(|_parse_err| anyhow::anyhow!("invalid datetime '{s}'; expected YYYY-MM-DD HH:MM"))
}
