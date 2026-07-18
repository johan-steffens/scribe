//! Selection, navigation, and utility helpers for the key handler.
//!
//! These are pure helper functions that read or compute values from `App`
//! state without performing any mutations or I/O.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime, TimeZone};

use crate::domain::{Links, Task, TaskPriority};
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

    // Load note links when entering Notes view.
    if view == View::Notes {
        app.refresh_notes();
        load_note_links(app);
    }
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
        if app.active_view == View::Notes {
            load_note_links(app);
        }
    }
}

/// Moves the selection cursor up by one in the active list view.
pub(super) fn move_selection_up(app: &mut App) {
    let sel = app.selected_mut();
    if *sel > 0 {
        *sel -= 1;
        if app.active_view == View::Notes {
            load_note_links(app);
        }
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
    let visible = visible_entries(app);
    visible.get(app.entries.selected).copied()
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

/// Returns the currently selected visible note, if any.
pub(super) fn selected_note(app: &App) -> Option<crate::domain::Note> {
    let filter = app.notes.filter.to_lowercase();
    let visible: Vec<_> = app
        .notes
        .items
        .iter()
        .filter(|n| {
            filter.is_empty()
                || n.slug.to_lowercase().contains(&filter)
                || n.title.to_lowercase().contains(&filter)
        })
        .collect();
    visible.get(app.notes.selected).map(|note| (*note).clone())
}

/// Loads the inbound backlinks for the currently selected note into `app.note_links`.
pub(super) fn load_note_links(app: &mut App) {
    let Some(note) = selected_note(app) else {
        app.note_links.clear();
        return;
    };
    let links_store = crate::store::SqliteLinks::new(Arc::clone(&app.db));
    match links_store.inbound_for(&note.slug) {
        Ok(links) => {
            app.note_links = links;
        }
        Err(e) => {
            app.last_error = Some(format!("failed to load links: {e}"));
            app.note_links.clear();
        }
    }
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
pub(super) fn selected_task(app: &App) -> Option<Task> {
    visible_task_at_index(app, app.tasks.selected)
}

// ── misc utilities ─────────────────────────────────────────────────────────

/// Returns a mutable reference to the filter string for the active view.
pub(super) fn current_filter_mut(app: &mut App) -> &mut String {
    match app.active_view {
        View::Projects => &mut app.projects.filter,
        // Dashboard reuses the tasks filter for title/project match on due list.
        View::Tasks | View::Dashboard => &mut app.tasks.filter,
        View::Todos => &mut app.todos.filter,
        View::Tracker => &mut app.entries.filter,
        View::Inbox => &mut app.captures.filter,
        View::Reminders => &mut app.reminders.filter,
        View::Notes => &mut app.notes.filter,
    }
}

/// Whether a task is due today or overdue and still actionable on the dashboard.
#[must_use]
pub(crate) fn is_dashboard_due_task(task: &Task, today: chrono::NaiveDate) -> bool {
    use crate::domain::task::TaskStatus;
    task.archived_at.is_none()
        && task.status != TaskStatus::Done
        && task.status != TaskStatus::Cancelled
        && task.due_date.is_some_and(|d| d <= today)
}

/// Due-today / overdue tasks for the dashboard, filtered and urgent-first.
#[must_use]
pub(crate) fn dashboard_due_tasks(app: &App) -> Vec<Task> {
    let today = chrono::Local::now().date_naive();
    let filter = app.tasks.filter.to_lowercase();
    let mut due: Vec<Task> = app
        .tasks
        .items
        .iter()
        .filter(|t| is_dashboard_due_task(t, today))
        .filter(|t| {
            filter.is_empty()
                || t.title.to_lowercase().contains(&filter)
                || t.project_slug.to_lowercase().contains(&filter)
        })
        .cloned()
        .collect();
    due.sort_by_key(|t| match t.priority {
        TaskPriority::Urgent => 0u8,
        TaskPriority::High => 1,
        TaskPriority::Medium => 2,
        TaskPriority::Low => 3,
    });
    due
}

/// Count of tasks shown in the dashboard due list.
#[must_use]
pub(crate) fn dashboard_due_task_count(app: &App) -> usize {
    dashboard_due_tasks(app).len()
}

/// The currently selected dashboard due task, if any.
#[must_use]
pub(super) fn selected_dashboard_task(app: &App) -> Option<Task> {
    let due = dashboard_due_tasks(app);
    due.get(app.dashboard_selected).cloned()
}

/// Whether a time entry matches the tracker filter string.
fn entry_matches_filter(entry: &crate::domain::TimeEntry, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let f = filter.to_lowercase();
    entry.slug.to_lowercase().contains(&f)
        || entry
            .note
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(&f)
}

/// Visible (filtered) tracker entries in list order.
#[must_use]
pub(crate) fn visible_entries(app: &App) -> Vec<&crate::domain::TimeEntry> {
    let filter = app.entries.filter.to_lowercase();
    app.entries
        .items
        .iter()
        .filter(|e| entry_matches_filter(e, &filter))
        .collect()
}

/// Truncates `s` to at most `max_chars` Unicode scalar values, appending `…`
/// when truncated. Safe for multi-byte UTF-8 (unlike byte slicing).
#[must_use]
pub(crate) fn truncate_chars(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_owned();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}…")
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

/// Parses an optional task due date (`YYYY-MM-DD`).
///
/// Empty / whitespace-only input yields `None`.
///
/// # Errors
///
/// Returns an error if the string is non-empty and not a valid calendar date.
pub(super) fn parse_due_date(s: &str) -> anyhow::Result<Option<NaiveDate>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map(Some)
        .map_err(|_e| anyhow::anyhow!("invalid due date '{trimmed}'; expected YYYY-MM-DD"))
}

/// Maps a priority select index to [`TaskPriority`].
///
/// Index order must match the Priority select options in create/edit task forms
/// (`low`, `medium`, `high`, `urgent`).
#[must_use]
pub(super) fn priority_from_select(index: usize) -> TaskPriority {
    match index {
        0 => TaskPriority::Low,
        1 => TaskPriority::Medium,
        2 => TaskPriority::High,
        _ => TaskPriority::Urgent,
    }
}

/// Select index for a [`TaskPriority`] in create/edit task forms.
#[must_use]
pub(super) const fn priority_select_index(priority: TaskPriority) -> usize {
    match priority {
        TaskPriority::Low => 0,
        TaskPriority::Medium => 1,
        TaskPriority::High => 2,
        TaskPriority::Urgent => 3,
    }
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

/// Returns the number of visible tasks in the tree view.
pub(crate) fn visible_task_count(app: &App) -> usize {
    let filter = app.tasks.filter.to_lowercase();

    // Collect all tasks
    let all_tasks: Vec<&Task> = app
        .tasks
        .items
        .iter()
        .filter(|t| t.archived_at.is_none())
        .collect();

    // Build children map
    let mut children_map: HashMap<crate::domain::TaskId, Vec<&Task>> = HashMap::new();
    for task in &all_tasks {
        if let Some(parent_id) = task.parent_id {
            children_map.entry(parent_id).or_default().push(task);
        }
    }

    // Filter to top-level tasks
    let top_level: Vec<&Task> = all_tasks
        .iter()
        .filter(|t| t.parent_id.is_none())
        .filter(|t| {
            if filter.is_empty() {
                true
            } else {
                t.title.to_lowercase().contains(&filter)
                    || t.project_slug.to_lowercase().contains(&filter)
            }
        })
        .copied()
        .collect();

    // Count visible tasks
    let mut count = 0;
    for task in top_level {
        count += 1;
        if app.tasks.expanded_tasks.contains(&task.id) {
            count += count_visible_children(task.id, &children_map, &filter, app).unwrap_or(0);
        }
    }
    count
}

/// Returns the task at the given index in the visible tree, if any.
pub(super) fn visible_task_at_index(app: &App, index: usize) -> Option<Task> {
    let filter = app.tasks.filter.to_lowercase();

    // Collect all visible tasks in tree order
    let all_tasks: Vec<&Task> = app
        .tasks
        .items
        .iter()
        .filter(|t| t.archived_at.is_none())
        .collect();

    // Build children map
    let mut children_map: HashMap<crate::domain::TaskId, Vec<&Task>> = HashMap::new();
    for task in &all_tasks {
        if let Some(parent_id) = task.parent_id {
            children_map.entry(parent_id).or_default().push(task);
        }
    }

    // Filter to top-level tasks
    let top_level: Vec<&Task> = all_tasks
        .iter()
        .filter(|t| t.parent_id.is_none())
        .filter(|t| {
            if filter.is_empty() {
                true
            } else {
                t.title.to_lowercase().contains(&filter)
                    || t.project_slug.to_lowercase().contains(&filter)
            }
        })
        .copied()
        .collect();

    // Walk the tree to find the task at the given index
    let mut current_index = 0;
    for task in top_level {
        if current_index == index {
            return Some(task.clone());
        }
        current_index += 1;

        // If expanded, walk children
        if app.tasks.expanded_tasks.contains(&task.id)
            && let Some(count) = count_visible_children(task.id, &children_map, &filter, app)
        {
            if current_index + count > index {
                // Task is in the children
                return find_task_in_children(
                    task.id,
                    index - current_index,
                    &children_map,
                    &filter,
                    app,
                );
            }
            current_index += count;
        }
    }

    None
}

/// Counts visible children of a task (recursively).
pub(super) fn count_visible_children(
    task_id: crate::domain::TaskId,
    children_map: &HashMap<crate::domain::TaskId, Vec<&Task>>,
    filter: &str,
    app: &App,
) -> Option<usize> {
    let children = children_map.get(&task_id)?;
    let mut count = 0;
    for child in children {
        if filter.is_empty()
            || child.title.to_lowercase().contains(filter)
            || child.project_slug.to_lowercase().contains(filter)
        {
            count += 1;
            if app.tasks.expanded_tasks.contains(&child.id) {
                count += count_visible_children(child.id, children_map, filter, app).unwrap_or(0);
            }
        }
    }
    Some(count)
}

/// Finds a task in the children of a parent task.
pub(super) fn find_task_in_children(
    parent_id: crate::domain::TaskId,
    target_index: usize,
    children_map: &HashMap<crate::domain::TaskId, Vec<&Task>>,
    filter: &str,
    app: &App,
) -> Option<Task> {
    let children = children_map.get(&parent_id)?;
    let mut current_index = 0;
    for child in children {
        if filter.is_empty()
            || child.title.to_lowercase().contains(filter)
            || child.project_slug.to_lowercase().contains(filter)
        {
            if current_index == target_index {
                return Some((*child).clone());
            }
            current_index += 1;

            if app.tasks.expanded_tasks.contains(&child.id)
                && let Some(count) = count_visible_children(child.id, children_map, filter, app)
            {
                if current_index + count > target_index {
                    return find_task_in_children(
                        child.id,
                        target_index - current_index,
                        children_map,
                        filter,
                        app,
                    );
                }
                current_index += count;
            }
        }
    }
    None
}

/// Finds the visible index of a task by its ID.
pub(super) fn find_visible_parent_index(
    app: &App,
    parent_id: crate::domain::TaskId,
) -> Option<usize> {
    let filter = app.tasks.filter.to_lowercase();

    let all_tasks: Vec<&Task> = app
        .tasks
        .items
        .iter()
        .filter(|t| t.archived_at.is_none())
        .collect();

    let mut children_map: HashMap<crate::domain::TaskId, Vec<&Task>> = HashMap::new();
    for task in &all_tasks {
        if let Some(pid) = task.parent_id {
            children_map.entry(pid).or_default().push(task);
        }
    }

    let top_level: Vec<&Task> = all_tasks
        .iter()
        .filter(|t| t.parent_id.is_none())
        .filter(|t| {
            if filter.is_empty() {
                true
            } else {
                t.title.to_lowercase().contains(&filter)
                    || t.project_slug.to_lowercase().contains(&filter)
            }
        })
        .copied()
        .collect();

    let mut current_index = 0;
    for task in top_level {
        if task.id == parent_id {
            return Some(current_index);
        }
        current_index += 1;

        if app.tasks.expanded_tasks.contains(&task.id)
            && let Some(count) = count_visible_children(task.id, &children_map, &filter, app)
        {
            if current_index + count > current_index
                && let Some(idx) = find_in_children(
                    task.id,
                    parent_id,
                    current_index,
                    &children_map,
                    &filter,
                    app,
                )
            {
                return Some(idx);
            }
            current_index += count;
        }
    }

    None
}

/// Finds a task in children recursively.
pub(super) fn find_in_children(
    parent_id: crate::domain::TaskId,
    target_id: crate::domain::TaskId,
    start_index: usize,
    children_map: &HashMap<crate::domain::TaskId, Vec<&Task>>,
    filter: &str,
    app: &App,
) -> Option<usize> {
    let children = children_map.get(&parent_id)?;
    let mut current_index = start_index;
    for child in children {
        if filter.is_empty()
            || child.title.to_lowercase().contains(filter)
            || child.project_slug.to_lowercase().contains(filter)
        {
            if child.id == target_id {
                return Some(current_index);
            }
            current_index += 1;

            if app.tasks.expanded_tasks.contains(&child.id)
                && let Some(count) = count_visible_children(child.id, children_map, filter, app)
            {
                if current_index + count > current_index
                    && let Some(idx) = find_in_children(
                        child.id,
                        target_id,
                        current_index,
                        children_map,
                        filter,
                        app,
                    )
                {
                    return Some(idx);
                }
                current_index += count;
            }
        }
    }
    None
}
