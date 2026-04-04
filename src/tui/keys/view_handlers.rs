//! Per-view action handlers triggered by `n`/`e`/`D`/`Space`/`Enter`/`v`.
//!
//! Each handler opens a form or confirm dialog by writing into `app.modal`.
//! No ops-layer calls are made here — mutations happen in [`super::actions`]
//! after the modal is submitted.
//!
//! Form-building logic for create/edit is factored into [`super::forms`].

use std::sync::Arc;

use crate::domain::TaskPatch;
use crate::domain::task::TaskStatus;
use crate::ops::tasks::TaskOps;
use crate::ops::todos::TodoOps;
use crate::ops::tracker::TrackerOps;
use crate::tui::app::App;
use crate::tui::components::dialog::ConfirmDialog;
use crate::tui::components::form::{Form, FormField};
use crate::tui::types::{ConfirmContext, FormContext, Modal, View};

use super::forms::{build_create_form, build_edit_form};
use super::helpers::{
    project_slugs, selected_capture, selected_entry, selected_project, selected_reminder,
    selected_task, selected_todo,
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
    if let Some((form, ctx)) = build_edit_form(app) {
        app.modal = Modal::Form(form, ctx);
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
        View::Dashboard => return,
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
        View::Dashboard | View::Projects | View::Inbox | View::Reminders => {}
    }
}

/// Handles `Enter` — process inbox item or detail view.
pub(super) fn handle_enter(app: &mut App) {
    if app.active_view != View::Inbox {
        return;
    }
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
