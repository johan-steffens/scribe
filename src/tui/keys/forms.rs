//! Form-building helpers for the create/edit view handlers.
//!
//! Each `build_*_form` function returns an `Option<(Form, FormContext)>` so
//! that the caller in [`super::view_handlers`] stays concise. `None` means
//! the current view has no create/edit form (e.g. Dashboard, Inbox).

use crate::tui::app::App;
use crate::tui::components::form::{Form, FormField};
use crate::tui::types::{FormContext, View};

use super::helpers::{
    project_slugs, selected_entry, selected_project, selected_reminder, selected_task,
    selected_todo,
};

// ── create forms ─────────────────────────────────────────────────────────

/// Builds a create-form and context for the active view.
///
/// Returns `None` when the current view has no create action (Dashboard).
#[expect(
    clippy::too_many_lines,
    reason = "form builders for 7 views are inherently verbose; splitting further would reduce cohesion"
)]
pub(super) fn build_create_form(app: &App) -> Option<(Form, FormContext)> {
    let project_options = project_slugs(app);
    match app.active_view {
        View::Todos => Some((
            Form::new(
                "New Todo",
                vec![
                    FormField::Text {
                        label: "Title".into(),
                        value: String::new(),
                        placeholder: "Enter todo title…".into(),
                        cursor: 0,
                    },
                    FormField::Select {
                        label: "Project".into(),
                        options: project_options,
                        selected: 0,
                    },
                ],
            ),
            FormContext::CreateTodo,
        )),
        View::Tracker => Some((
            Form::new(
                "Start Timer",
                vec![
                    FormField::Select {
                        label: "Project".into(),
                        options: project_options,
                        selected: 0,
                    },
                    FormField::Text {
                        label: "Note (optional)".into(),
                        value: String::new(),
                        placeholder: "What are you working on?".into(),
                        cursor: 0,
                    },
                ],
            ),
            FormContext::StartTimer,
        )),
        View::Inbox => Some((
            Form::new(
                "Capture",
                vec![FormField::Text {
                    label: "Body".into(),
                    value: String::new(),
                    placeholder: "What's on your mind?".into(),
                    cursor: 0,
                }],
            ),
            FormContext::CreateCapture,
        )),
        View::Reminders => Some((
            Form::new(
                "New Reminder",
                vec![
                    FormField::Select {
                        label: "Project".into(),
                        options: project_options,
                        selected: 0,
                    },
                    FormField::DateTime {
                        label: "Remind at (YYYY-MM-DD HH:MM)".into(),
                        value: String::new(),
                        error: None,
                        cursor: 0,
                    },
                    FormField::Text {
                        label: "Message (optional)".into(),
                        value: String::new(),
                        placeholder: "Reminder message…".into(),
                        cursor: 0,
                    },
                    FormField::Select {
                        label: "Notification style".into(),
                        options: vec![
                            "Banner (auto-dismiss)".into(),
                            "Alert (stay until dismissed)".into(),
                        ],
                        selected: 0,
                    },
                ],
            ),
            FormContext::CreateReminder,
        )),
        View::Projects => Some((
            Form::new(
                "New Project",
                vec![
                    FormField::Text {
                        label: "Slug (kebab-case)".into(),
                        value: String::new(),
                        placeholder: "my-project".into(),
                        cursor: 0,
                    },
                    FormField::Text {
                        label: "Name".into(),
                        value: String::new(),
                        placeholder: "My Project".into(),
                        cursor: 0,
                    },
                ],
            ),
            FormContext::CreateProject,
        )),
        View::Tasks => Some((
            Form::new(
                "New Task",
                vec![
                    FormField::Text {
                        label: "Title".into(),
                        value: String::new(),
                        placeholder: "Task title…".into(),
                        cursor: 0,
                    },
                    FormField::Select {
                        label: "Project".into(),
                        options: project_options,
                        selected: 0,
                    },
                ],
            ),
            FormContext::CreateTask,
        )),
        View::Dashboard => None,
    }
}

// ── edit forms ────────────────────────────────────────────────────────────

/// Builds an edit-form and context for the selected item in the active view.
///
/// Returns `None` when the current view has no edit action (Dashboard, Inbox).
#[expect(
    clippy::too_many_lines,
    reason = "edit-form builders for 5 views with pre-fill logic are inherently verbose"
)]
pub(super) fn build_edit_form(app: &App) -> Option<(Form, FormContext)> {
    let project_options = project_slugs(app);
    match app.active_view {
        View::Todos => {
            let todo = selected_todo(app)?;
            let title = todo.title.clone();
            let slug = todo.slug.clone();
            let proj_idx = project_options
                .iter()
                .position(|s| {
                    app.projects
                        .items
                        .iter()
                        .find(|p| p.slug == *s)
                        .is_some_and(|p| p.id == todo.project_id)
                })
                .unwrap_or(0);
            Some((
                Form::new(
                    "Edit Todo",
                    vec![
                        FormField::Text {
                            label: "Title".into(),
                            value: title.clone(),
                            placeholder: String::new(),
                            cursor: title.len(),
                        },
                        FormField::Select {
                            label: "Project".into(),
                            options: project_options,
                            selected: proj_idx,
                        },
                    ],
                ),
                FormContext::EditTodo(slug),
            ))
        }
        View::Tracker => {
            let entry = selected_entry(app)?;
            let note = entry.note.clone().unwrap_or_default();
            let slug = entry.slug.clone();
            let cursor = note.len();
            Some((
                Form::new(
                    "Edit Note",
                    vec![FormField::Text {
                        label: "Note".into(),
                        value: note,
                        placeholder: "Entry note…".into(),
                        cursor,
                    }],
                ),
                FormContext::EditEntryNote(slug),
            ))
        }
        View::Reminders => {
            let reminder = selected_reminder(app)?;
            let slug = reminder.slug.clone();
            let remind_at = reminder.remind_at.format("%Y-%m-%d %H:%M").to_string();
            let message = reminder.message.clone().unwrap_or_default();
            let cursor = remind_at.len();
            // DOCUMENTED-MAGIC: persistent index 1 = "Alert (stay until dismissed)".
            let persistent_selected = usize::from(reminder.persistent);
            Some((
                Form::new(
                    "Edit Reminder",
                    vec![
                        FormField::DateTime {
                            label: "Remind at (YYYY-MM-DD HH:MM)".into(),
                            value: remind_at,
                            error: None,
                            cursor,
                        },
                        FormField::Text {
                            label: "Message".into(),
                            value: message.clone(),
                            placeholder: String::new(),
                            cursor: message.len(),
                        },
                        FormField::Select {
                            label: "Notification style".into(),
                            options: vec![
                                "Banner (auto-dismiss)".into(),
                                "Alert (stay until dismissed)".into(),
                            ],
                            selected: persistent_selected,
                        },
                    ],
                ),
                FormContext::EditReminder(slug),
            ))
        }
        View::Projects => {
            let project = selected_project(app)?;
            let name = project.name.clone();
            let slug = project.slug.clone();
            let cursor = name.len();
            Some((
                Form::new(
                    "Edit Project",
                    vec![FormField::Text {
                        label: "Name".into(),
                        value: name,
                        placeholder: String::new(),
                        cursor,
                    }],
                ),
                FormContext::EditProject(slug),
            ))
        }
        View::Tasks => {
            let task = selected_task(app)?;
            let title = task.title.clone();
            let slug = task.slug.clone();
            let cursor = title.len();
            Some((
                Form::new(
                    "Edit Task",
                    vec![FormField::Text {
                        label: "Title".into(),
                        value: title,
                        placeholder: String::new(),
                        cursor,
                    }],
                ),
                FormContext::EditTask(slug),
            ))
        }
        View::Dashboard | View::Inbox => None,
    }
}
