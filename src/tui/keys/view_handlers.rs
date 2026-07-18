//! Per-view action handlers triggered by `n`/`e`/`D`/`Space`/`Enter`/`v`.
//!
//! Each handler opens a form or confirm dialog by writing into `app.modal`.
//! No ops-layer calls are made here — mutations happen in [`super::actions`]
//! after the modal is submitted.
//!
//! Form-building logic for create/edit is factored into [`super::forms`].

use std::sync::Arc;

use crate::domain::task::TaskStatus;
use crate::domain::{TaskPatch, Tasks};
use crate::ops::notes::NotesOps;
use crate::ops::tasks::TaskOps;
use crate::ops::todos::TodoOps;
use crate::ops::tracker::TrackerOps;
use crate::store::{SqliteLinks, SqliteNotes, SqliteTasks};
use crate::tui::app::App;
use crate::tui::components::dialog::ConfirmDialog;
use crate::tui::components::form::{Form, FormField};
use crate::tui::types::{ConfirmContext, FormContext, Modal, View};

use super::forms::{build_create_form, build_edit_form};
use super::helpers::{
    find_visible_parent_index, project_slugs, selected_capture, selected_entry, selected_note,
    selected_project, selected_reminder, selected_task, selected_todo, visible_task_at_index,
    visible_task_count,
};

// ── create ────────────────────────────────────────────────────────────────

/// Opens a create form for the active view.
pub(super) fn handle_new(app: &mut App) {
    if let Some((form, ctx)) = build_create_form(app) {
        app.modal = Modal::Form(form, ctx);
    }
}

// ── edit ──────────────────────────────────────────────────────────────────

/// Opens an edit form for the selected item.
pub(super) fn handle_edit(app: &mut App) {
    if app.active_view == View::Notes {
        handle_edit_note(app);
        return;
    }
    if let Some((form, ctx)) = build_edit_form(app) {
        app.modal = Modal::Form(form, ctx);
    }
}

/// Handles `e` in the Notes view — opens the note in `$EDITOR`.
fn handle_edit_note(app: &mut App) {
    let Some(note) = selected_note(app) else {
        return;
    };
    let slug = note.slug.clone();
    let ops = NotesOps::new(
        Arc::new(SqliteNotes::new(Arc::clone(&app.db))),
        Arc::new(SqliteLinks::new(Arc::clone(&app.db))),
        app.note_editor.clone(),
    );
    match ops.edit_note(&slug) {
        Ok(_) => {
            app.refresh();
        }
        Err(e) => {
            app.last_error = Some(e.to_string());
        }
    }
}

/// Handles `g` in the Notes view — jumps to a linked task.
pub(super) fn handle_go_to_linked(app: &mut App) {
    if app.active_view != View::Notes {
        return;
    }
    // If there are no backlinks, do nothing.
    if app.note_links.is_empty() {
        return;
    }
    // Get the first backlink's source slug and try to find it as a task.
    // Clone the slug here to avoid borrow checker issues.
    let source_slug = {
        let Some(link) = app.note_links.first() else {
            return;
        };
        link.source_slug.clone()
    };

    // Switch to Tasks view and try to find the task.
    super::helpers::switch_view(app, View::Tasks);

    // Search for a task with this slug.
    let task_store = SqliteTasks::new(Arc::clone(&app.db));
    match task_store.find_by_slug(&source_slug) {
        Ok(Some(task)) => {
            // Find the index of this task in the visible list.
            let filter = app.tasks.filter.to_lowercase();
            let visible: Vec<_> = app
                .tasks
                .items
                .iter()
                .filter(|t| t.archived_at.is_none())
                .filter(|t| {
                    if filter.is_empty() {
                        true
                    } else {
                        t.title.to_lowercase().contains(&filter)
                            || t.project_slug.to_lowercase().contains(&filter)
                    }
                })
                .collect();
            if let Some(idx) = visible.iter().position(|t| t.id == task.id) {
                app.tasks.selected = idx;
            }
        }
        Ok(None) => {
            // Task not found — stay in Tasks view, selection unchanged.
            app.last_error = Some(format!("no task found with slug '{source_slug}'"));
        }
        Err(e) => {
            app.last_error = Some(e.to_string());
        }
    }
}

// ── delete ─────────────────────────────────────────────────────────────────

/// Opens a delete/archive confirmation dialog for the selected item.
pub(super) fn handle_delete(app: &mut App) {
    let ctx = match app.active_view {
        View::Todos => {
            let Some(todo) = selected_todo(app) else {
                return;
            };
            ConfirmContext::ArchiveTodo(todo.slug.clone())
        }
        View::Tracker => {
            let Some(entry) = selected_entry(app) else {
                return;
            };
            ConfirmContext::ArchiveEntry(entry.slug.clone())
        }
        View::Inbox => {
            let Some(capture) = selected_capture(app) else {
                return;
            };
            ConfirmContext::DeleteCapture(capture.slug.clone())
        }
        View::Reminders => {
            let Some(reminder) = selected_reminder(app) else {
                return;
            };
            ConfirmContext::ArchiveReminder(reminder.slug.clone())
        }
        View::Projects => {
            let Some(project) = selected_project(app) else {
                return;
            };
            ConfirmContext::ArchiveProject(project.slug.clone())
        }
        View::Tasks => {
            let Some(task) = selected_task(app) else {
                return;
            };
            ConfirmContext::ArchiveTask(task.slug.clone())
        }
        View::Notes | View::Dashboard => return,
    };

    let msg = match &ctx {
        ConfirmContext::ArchiveTodo(_) => "Archive this todo?",
        ConfirmContext::ArchiveEntry(_) => "Archive this time entry?",
        ConfirmContext::DeleteCapture(_) => "Delete this capture item?",
        ConfirmContext::ArchiveReminder(_) => "Archive this reminder?",
        ConfirmContext::ArchiveProject(_) => "Archive this project (and all its items)?",
        ConfirmContext::ArchiveTask(_) => "Archive this task?",
    };

    app.modal = Modal::Confirm(ConfirmDialog::new(msg), ctx);
}

// ── space / enter / move ──────────────────────────────────────────────────

/// Handles `Space` — primary action per view.
pub(super) fn handle_space(app: &mut App) {
    match app.active_view {
        View::Todos => toggle_todo_done(app),
        View::Tasks => toggle_task_done(app),
        View::Tracker => handle_tracker_space(app),
        View::Dashboard | View::Projects | View::Inbox | View::Reminders | View::Notes => {}
    }
}

/// Handles `Enter` — process inbox item or detail view, or expand/collapse task.
pub(super) fn handle_enter(app: &mut App) {
    match app.active_view {
        View::Inbox => handle_inbox_enter(app),
        View::Tasks => toggle_task_expand(app),
        _ => {}
    }
}

/// Handles `Enter` in the inbox view — opens process capture form.
fn handle_inbox_enter(app: &mut App) {
    let Some(capture) = selected_capture(app) else {
        return;
    };
    let body = capture.body.clone();
    let slug = capture.slug.clone();
    let title = format!("Process: {}", &body[..body.len().min(40)]);
    let form = Form::new(
        title,
        vec![
            FormField::Select {
                label: "Action".into(),
                options: vec![
                    "Convert to Todo".into(),
                    "Assign to Project".into(),
                    "Discard".into(),
                ],
                selected: 0,
            },
            FormField::Select {
                label: "Project".into(),
                options: project_slugs(app),
                selected: 0,
            },
        ],
    );
    app.modal = Modal::Form(form, FormContext::ProcessCapture(slug));
}

/// Toggles the expanded/collapsed state of the selected task.
fn toggle_task_expand(app: &mut App) {
    let Some(task) = visible_task_at_index(app, app.tasks.selected) else {
        return;
    };
    let task_id = task.id;
    if app.tasks.expanded_tasks.contains(&task_id) {
        app.tasks.expanded_tasks.remove(&task_id);
    } else {
        app.tasks.expanded_tasks.insert(task_id);
    }
    // Clamp selection to valid range
    let max_idx = visible_task_count(app).saturating_sub(1);
    app.tasks.selected = app.tasks.selected.min(max_idx);
}

/// Handles `Right` arrow — expand the selected task.
pub(super) fn handle_right(app: &mut App) {
    if app.active_view != View::Tasks {
        return;
    }
    let Some(task) = visible_task_at_index(app, app.tasks.selected) else {
        return;
    };
    // Only expand if the task has children
    let has_children = app
        .tasks
        .items
        .iter()
        .any(|t| t.parent_id == Some(task.id) && t.archived_at.is_none());
    if has_children {
        app.tasks.expanded_tasks.insert(task.id);
    }
    // Clamp selection to valid range
    let max_idx = visible_task_count(app).saturating_sub(1);
    app.tasks.selected = app.tasks.selected.min(max_idx);
}

/// Handles `Left` arrow — collapse the selected task, or move to parent.
pub(super) fn handle_left(app: &mut App) {
    if app.active_view != View::Tasks {
        return;
    }
    let Some(task) = visible_task_at_index(app, app.tasks.selected) else {
        return;
    };
    // If the task is expanded, collapse it
    if app.tasks.expanded_tasks.contains(&task.id) {
        app.tasks.expanded_tasks.remove(&task.id);
    } else if let Some(parent_id) = task.parent_id {
        // Move selection to parent task
        if let Some(parent_index) = find_visible_parent_index(app, parent_id) {
            app.tasks.selected = parent_index;
        }
    }
    // Clamp selection to valid range
    let max_idx = visible_task_count(app).saturating_sub(1);
    app.tasks.selected = app.tasks.selected.min(max_idx);
}

/// Handles the `v` key (move todo to a different project).
pub(super) fn handle_move_todo(app: &mut App) {
    let Some(todo) = selected_todo(app) else {
        return;
    };
    let slug = todo.slug.clone();
    let project_options = project_slugs(app);
    let form = Form::new(
        "Move Todo",
        vec![FormField::Select {
            label: "Destination Project".into(),
            options: project_options,
            selected: 0,
        }],
    );
    app.modal = Modal::Form(form, FormContext::MoveTodo(slug));
}

// ── private helpers ───────────────────────────────────────────────────────

/// Toggles the done state of the selected todo.
fn toggle_todo_done(app: &mut App) {
    let Some(todo) = selected_todo(app) else {
        return;
    };
    let slug = todo.slug.clone();
    let done = todo.done;
    let ops = TodoOps::new(Arc::clone(&app.db));
    let result = if done {
        ops.mark_undone(&slug).map(|_| ())
    } else {
        ops.mark_done(&slug).map(|_| ())
    };
    match result {
        Ok(()) => app.refresh(),
        Err(e) => app.last_error = Some(e.to_string()),
    }
}

/// Toggles the done status of the selected task.
fn toggle_task_done(app: &mut App) {
    let Some(task) = selected_task(app) else {
        return;
    };
    let slug = task.slug.clone();
    let is_done = task.status == TaskStatus::Done;
    let ops = TaskOps::new(Arc::clone(&app.db));
    let result = if is_done {
        ops.update_task(
            &slug,
            TaskPatch {
                status: Some(TaskStatus::Todo),
                ..Default::default()
            },
        )
        .map(|_| ())
    } else {
        ops.mark_done(&slug).map(|_| ())
    };
    match result {
        Ok(()) => app.refresh(),
        Err(e) => app.last_error = Some(e.to_string()),
    }
}

/// Handles Space in the Tracker view: stop if running, or open start-timer form.
fn handle_tracker_space(app: &mut App) {
    let tracker = TrackerOps::new(Arc::clone(&app.db));
    if app.active_timer.is_some() {
        match tracker.stop_timer() {
            Ok(_) => app.refresh(),
            Err(e) => app.last_error = Some(e.to_string()),
        }
    } else {
        handle_new(app);
    }
}
