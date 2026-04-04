//! Ops-layer mutations called after form submission or dialog confirmation.
//!
//! Each `exec_*` function corresponds to one [`FormContext`] variant and calls
//! the appropriate `ops` function, then signals the result to the caller via
//! `anyhow::Result<()>`.
//!
//! [`execute_confirm`] and [`execute_form`] are the two entry points called
//! from the modal key handler.

use std::sync::Arc;

use crate::domain::Projects;
use crate::ops::reminders::CreateReminder;
use crate::ops::todos::TodoOps;
use crate::ops::tracker::{StartTimer, TrackerOps};
use crate::ops::{InboxOps, ProjectOps, ReminderOps, TaskOps};
use crate::store::SqliteProjects;
use crate::tui::app::App;
use crate::tui::components::form::Form;
use crate::tui::types::{ConfirmContext, FormContext};

// ── confirm actions ────────────────────────────────────────────────────────

/// Executes the mutation associated with a confirmed dialog.
pub(super) fn execute_confirm(app: &mut App, ctx: &ConfirmContext) {
    let result = match ctx {
        ConfirmContext::ArchiveTodo(slug) => {
            TodoOps::new(Arc::clone(&app.db)).archive(slug).map(|_| ())
        }
        ConfirmContext::ArchiveEntry(slug) => TrackerOps::new(Arc::clone(&app.db))
            .archive_entry(slug)
            .map(|_| ()),
        ConfirmContext::DeleteCapture(slug) => {
            use crate::domain::CaptureItems;
            crate::store::SqliteCaptureItems::new(Arc::clone(&app.db)).delete(slug)
        }
        ConfirmContext::ArchiveReminder(slug) => ReminderOps::new(Arc::clone(&app.db))
            .archive(slug)
            .map(|_| ()),
        ConfirmContext::ArchiveProject(slug) => ProjectOps::new(&Arc::clone(&app.db))
            .archive_project(slug)
            .map(|_| ()),
        ConfirmContext::ArchiveTask(slug) => TaskOps::new(Arc::clone(&app.db))
            .archive_task(slug)
            .map(|_| ()),
    };
    apply_result(app, result);
}

// ── form actions ───────────────────────────────────────────────────────────

/// Executes the mutation after a form has been submitted.
pub(super) fn execute_form(app: &mut App, form: &Form, ctx: FormContext) {
    let result = match ctx {
        FormContext::CreateTodo => exec_create_todo(app, form),
        FormContext::EditTodo(slug) => exec_edit_todo(app, form, &slug),
        FormContext::MoveTodo(slug) => exec_move_todo(app, form, &slug),
        FormContext::StartTimer => exec_start_timer(app, form),
        FormContext::EditEntryNote(slug) => exec_edit_entry_note(app, form, &slug),
        FormContext::CreateCapture => exec_create_capture(app, form),
        FormContext::CreateReminder => exec_create_reminder(app, form),
        FormContext::EditReminder(slug) => exec_edit_reminder(app, form, &slug),
        FormContext::CreateProject => exec_create_project(app, form),
        FormContext::EditProject(slug) => exec_edit_project(app, form, &slug),
        FormContext::CreateTask => exec_create_task(app, form),
        FormContext::EditTask(slug) => exec_edit_task(app, form, &slug),
        FormContext::ProcessCapture(slug) => exec_process_capture(app, form, &slug),
    };
    apply_result(app, result);
}

// ── private exec helpers ───────────────────────────────────────────────────

fn apply_result(app: &mut App, result: anyhow::Result<()>) {
    match result {
        Ok(()) => app.refresh(),
        Err(e) => app.last_error = Some(e.to_string()),
    }
}

fn exec_create_todo(app: &App, form: &Form) -> anyhow::Result<()> {
    let title = form.field_value(0).to_owned();
    if title.is_empty() {
        return Err(anyhow::anyhow!("title cannot be empty"));
    }
    let project_slug = form.field_value(1).to_owned();
    TodoOps::new(Arc::clone(&app.db))
        .create(&project_slug, &title)
        .map(|_| ())
}

fn exec_edit_todo(app: &App, form: &Form, slug: &str) -> anyhow::Result<()> {
    let title = form.field_value(0).to_owned();
    if title.is_empty() {
        return Err(anyhow::anyhow!("title cannot be empty"));
    }
    TodoOps::new(Arc::clone(&app.db))
        .update_title(slug, &title)
        .map(|_| ())
}

fn exec_move_todo(app: &App, form: &Form, slug: &str) -> anyhow::Result<()> {
    let project_slug = form.field_value(0).to_owned();
    TodoOps::new(Arc::clone(&app.db))
        .move_project(slug, &project_slug)
        .map(|_| ())
}

fn exec_start_timer(app: &mut App, form: &Form) -> anyhow::Result<()> {
    let project_slug = form.field_value(0).to_owned();
    if project_slug.is_empty() {
        return Err(anyhow::anyhow!("project is required to start a timer"));
    }
    let note_raw = form.field_value(1);
    let note = if note_raw.is_empty() {
        None
    } else {
        Some(note_raw.to_owned())
    };
    let ops = TrackerOps::new(Arc::clone(&app.db));
    ops.resolve_project(&project_slug).and_then(|(slug, pid)| {
        ops.start_timer(StartTimer {
            project_slug: slug,
            project_id: pid,
            task_id: None,
            note,
        })
        .map(|_| ())
    })
}

fn exec_edit_entry_note(app: &App, form: &Form, slug: &str) -> anyhow::Result<()> {
    let note_raw = form.field_value(0);
    let note = if note_raw.is_empty() {
        None
    } else {
        Some(note_raw.to_owned())
    };
    TrackerOps::new(Arc::clone(&app.db))
        .update_note(slug, note)
        .map(|_| ())
}

fn exec_create_capture(app: &App, form: &Form) -> anyhow::Result<()> {
    let body = form.field_value(0).to_owned();
    if body.is_empty() {
        return Err(anyhow::anyhow!("capture body cannot be empty"));
    }
    InboxOps::new(&Arc::clone(&app.db))
        .capture(&body)
        .map(|_| ())
}

fn exec_create_reminder(app: &App, form: &Form) -> anyhow::Result<()> {
    let project_slug = form.field_value(0).to_owned();
    let remind_at_raw = form.field_value(1).to_owned();
    let message_raw = form.field_value(2);
    let message = if message_raw.is_empty() {
        None
    } else {
        Some(message_raw.to_owned())
    };
    // DOCUMENTED-MAGIC: field 3 is the "Notification style" Select:
    // index 0 = Banner (persistent = false), index 1 = Alert (persistent = true).
    let persistent = form.select_index(3) == 1;
    let remind_at = super::helpers::parse_datetime(&remind_at_raw)?;
    ReminderOps::new(Arc::clone(&app.db))
        .create(CreateReminder {
            project_slug,
            task_slug: None,
            remind_at,
            message,
            persistent,
        })
        .map(|_| ())
}

fn exec_edit_reminder(app: &App, form: &Form, slug: &str) -> anyhow::Result<()> {
    let remind_at_raw = form.field_value(0).to_owned();
    let message_raw = form.field_value(1);
    let message = if message_raw.is_empty() {
        None
    } else {
        Some(message_raw.to_owned())
    };
    let remind_at = if remind_at_raw.is_empty() {
        None
    } else {
        Some(super::helpers::parse_datetime(&remind_at_raw)?)
    };
    // DOCUMENTED-MAGIC: field 2 is "Notification style": 0=Banner, 1=Alert.
    let persistent = Some(form.select_index(2) == 1);
    ReminderOps::new(Arc::clone(&app.db))
        .update(
            slug,
            crate::domain::ReminderPatch {
                remind_at,
                message,
                persistent,
            },
        )
        .map(|_| ())
}

fn exec_create_project(app: &App, form: &Form) -> anyhow::Result<()> {
    let slug = form.field_value(0).to_owned();
    let name = form.field_value(1).to_owned();
    if slug.is_empty() || name.is_empty() {
        return Err(anyhow::anyhow!("slug and name are required"));
    }
    ProjectOps::new(&Arc::clone(&app.db))
        .create_project(crate::domain::NewProject {
            slug,
            name,
            description: None,
            status: crate::domain::ProjectStatus::Active,
        })
        .map(|_| ())
}

fn exec_edit_project(app: &App, form: &Form, project_slug: &str) -> anyhow::Result<()> {
    let name = form.field_value(0).to_owned();
    if name.is_empty() {
        return Err(anyhow::anyhow!("name cannot be empty"));
    }
    ProjectOps::new(&Arc::clone(&app.db))
        .update_project(
            project_slug,
            crate::domain::ProjectPatch {
                name: Some(name),
                ..Default::default()
            },
        )
        .map(|_| ())
}

fn exec_create_task(app: &App, form: &Form) -> anyhow::Result<()> {
    let title = form.field_value(0).to_owned();
    if title.is_empty() {
        return Err(anyhow::anyhow!("title cannot be empty"));
    }
    let project_slug = form.field_value(1).to_owned();
    let p = SqliteProjects::new(Arc::clone(&app.db))
        .find_by_slug(&project_slug)?
        .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;
    TaskOps::new(Arc::clone(&app.db))
        .create_task(crate::ops::tasks::CreateTask {
            project_slug: p.slug,
            project_id: p.id,
            title,
            description: None,
            status: crate::domain::TaskStatus::Todo,
            priority: crate::domain::TaskPriority::Medium,
            due_date: None,
        })
        .map(|_| ())
}

fn exec_edit_task(app: &App, form: &Form, task_slug: &str) -> anyhow::Result<()> {
    let title = form.field_value(0).to_owned();
    if title.is_empty() {
        return Err(anyhow::anyhow!("title cannot be empty"));
    }
    TaskOps::new(Arc::clone(&app.db))
        .update_task(
            task_slug,
            crate::domain::TaskPatch {
                title: Some(title),
                ..Default::default()
            },
        )
        .map(|_| ())
}

fn exec_process_capture(app: &App, form: &Form, capture_slug: &str) -> anyhow::Result<()> {
    let action_idx = form.select_index(0);
    let project_slug = form.field_value(1).to_owned();
    let ops = InboxOps::new(&Arc::clone(&app.db));
    let action = match action_idx {
        0 => crate::ops::inbox::ProcessAction::ConvertToTodo {
            project_slug,
            title: None,
        },
        1 => crate::ops::inbox::ProcessAction::AssignToProject { project_slug },
        _ => crate::ops::inbox::ProcessAction::Discard,
    };
    ops.process(capture_slug, action).map(|_| ())
}
