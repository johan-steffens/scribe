//! Data-refresh methods for [`App`].
//!
//! These private methods reload each view's data from the database. They are
//! split here to keep `app.rs` under the 400-line limit.

use std::sync::Arc;

use chrono::Utc;

use crate::domain::{CaptureItems, Projects, Reminders, Tasks, TimeEntries, Todos};
use crate::ops::ReportingOps;
use crate::store::{
    SqliteCaptureItems, SqliteProjects, SqliteReminders, SqliteTasks, SqliteTimeEntries,
    SqliteTodos,
};

use super::App;

/// Reloads the projects list.
pub(super) fn refresh_projects(app: &mut App) {
    let store = SqliteProjects::new(Arc::clone(&app.db));
    match store.list(None, false) {
        Ok(items) => {
            app.projects.items = items;
            clamp(&mut app.projects.selected, app.projects.items.len());
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load projects: {e}"));
        }
    }
}

/// Reloads the tasks list.
pub(super) fn refresh_tasks(app: &mut App) {
    let store = SqliteTasks::new(Arc::clone(&app.db));
    match store.list(None, None, None, false) {
        Ok(items) => {
            app.tasks.items = items;
            clamp(&mut app.tasks.selected, app.tasks.items.len());
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load tasks: {e}"));
        }
    }
}

/// Reloads the todos list (active, non-archived, including done).
pub(super) fn refresh_todos(app: &mut App) {
    let store = SqliteTodos::new(Arc::clone(&app.db));
    match store.list(None, true, false) {
        Ok(items) => {
            app.todos.items = items;
            clamp(&mut app.todos.selected, app.todos.items.len());
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load todos: {e}"));
        }
    }
}

/// Reloads the time-entries list (most recent 50, non-archived).
pub(super) fn refresh_entries(app: &mut App) {
    let store = SqliteTimeEntries::new(Arc::clone(&app.db));
    // DOCUMENTED-MAGIC: cap at 50 recent entries to keep the list
    // scrollable without overwhelming the TUI.
    match store.list(None, false) {
        Ok(items) => {
            app.entries.items = items.into_iter().take(50).collect();
            clamp(&mut app.entries.selected, app.entries.items.len());
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load entries: {e}"));
        }
    }
}

/// Reloads unprocessed capture items (oldest first).
pub(super) fn refresh_captures(app: &mut App) {
    let store = SqliteCaptureItems::new(Arc::clone(&app.db));
    match store.list(false) {
        Ok(mut items) => {
            // Oldest first per spec.
            items.sort_by_key(|c| c.created_at);
            app.captures.items = items;
            clamp(&mut app.captures.selected, app.captures.items.len());
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load captures: {e}"));
        }
    }
}

/// Reloads active (non-archived, non-fired) reminders.
pub(super) fn refresh_reminders(app: &mut App) {
    let store = SqliteReminders::new(Arc::clone(&app.db));
    match store.list(None, false) {
        Ok(items) => {
            // Only show non-fired reminders in the default view.
            let active: Vec<_> = items.into_iter().filter(|r| !r.fired).collect();
            app.reminders.items = active;
            clamp(&mut app.reminders.selected, app.reminders.items.len());
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load reminders: {e}"));
        }
    }
}

/// Reloads the summary report for the dashboard system overview.
pub(super) fn refresh_summary(app: &mut App) {
    let ops = ReportingOps::new(Arc::clone(&app.db));
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    match ops.summary_report(today_start, now) {
        Ok(summary) => {
            app.summary = Some(summary);
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load summary: {e}"));
        }
    }
}

// ── utilities ──────────────────────────────────────────────────────────────

/// Clamps `selected` to `[0, len.saturating_sub(1)]`.
pub(super) fn clamp(selected: &mut usize, len: usize) {
    let max = len.saturating_sub(1);
    if *selected > max {
        *selected = max;
    }
}
