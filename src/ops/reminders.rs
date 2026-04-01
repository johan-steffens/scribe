// Rust guideline compliant 2026-02-21
//! Business logic for the reminders feature.
//!
//! [`ReminderOps`] wraps `SqliteReminders` and adds project/task validation
//! on create, and the `check_due` workflow that fires due reminders.
//!
//! # TODO(phase3): migrate to domain error structs per M-ERRORS-CANONICAL-STRUCTS

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::domain::{NewReminder, Projects, Reminder, ReminderPatch, Reminders, Tasks, slug};
use crate::store::{SqliteProjects, SqliteReminders, SqliteTasks};

/// Parameters for creating a new reminder via [`ReminderOps`].
#[derive(Debug, Clone)]
pub struct CreateReminder {
    /// Owning project slug (used for slug prefix generation).
    pub project_slug: String,
    /// Optional linked task slug.
    pub task_slug: Option<String>,
    /// When the reminder should fire.
    pub remind_at: DateTime<Utc>,
    /// Optional free-text message to display.
    pub message: Option<String>,
    /// When `true`, the notification blocks until the user dismisses it.
    pub persistent: bool,
}

/// High-level reminder operations with project and task validation.
///
/// Construct via [`ReminderOps::new`], passing the shared database connection.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::ops::ReminderOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let ops = ReminderOps::new(conn);
/// ```
#[derive(Clone, Debug)]
pub struct ReminderOps {
    reminders: SqliteReminders,
    projects: SqliteProjects,
    tasks: SqliteTasks,
}

impl ReminderOps {
    /// Creates a new [`ReminderOps`] backed by the given connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::ops::ReminderOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let ops = ReminderOps::new(conn);
    /// ```
    #[must_use]
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            reminders: SqliteReminders::new(Arc::clone(&conn)),
            projects: SqliteProjects::new(Arc::clone(&conn)),
            tasks: SqliteTasks::new(conn),
        }
    }

    /// Creates a new reminder under the given project.
    ///
    /// Validates that the project exists and is not archived. If `task_slug`
    /// is provided, validates that the task also exists and is not archived.
    /// Slug format: `{project_slug}-reminder-{message_slug}[-{suffix}]`.
    ///
    /// # Errors
    ///
    /// Returns an error if the project or task does not exist, if either is
    /// archived, if slug generation fails, or a database error occurs.
    pub fn create(&self, params: CreateReminder) -> anyhow::Result<Reminder> {
        let project = self
            .projects
            .find_by_slug(&params.project_slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{}' not found", params.project_slug))?;

        if project.archived_at.is_some() {
            return Err(anyhow::anyhow!(
                "project '{}' is archived; restore it before adding reminders",
                params.project_slug,
            ));
        }

        let task_id = if let Some(ref task_slug) = params.task_slug {
            let task = self
                .tasks
                .find_by_slug(task_slug)?
                .ok_or_else(|| anyhow::anyhow!("task '{task_slug}' not found"))?;

            if task.archived_at.is_some() {
                return Err(anyhow::anyhow!(
                    "task '{task_slug}' is archived; restore it first"
                ));
            }
            Some(task.id)
        } else {
            None
        };

        // Use the message or the remind_at timestamp as the slug source.
        let slug_source = if let Some(ref msg) = params.message {
            msg.clone()
        } else {
            params.remind_at.format("%Y%m%d-%H%M%S").to_string()
        };
        let prefix = format!("{}-reminder-", params.project_slug);
        let base_slug = slug::generate(&prefix, &slug_source);
        let unique_slug = slug::ensure_unique(&base_slug, |candidate| {
            self.reminders
                .find_by_slug(candidate)
                .map(|r| r.is_some())
                .unwrap_or(false)
        })
        .map_err(|e| anyhow::anyhow!("slug generation failed: {e}"))?;

        self.reminders.create(NewReminder {
            slug: unique_slug,
            project_id: project.id,
            task_id,
            remind_at: params.remind_at,
            message: params.message,
            persistent: params.persistent,
        })
    }

    /// Returns the reminder with the given `slug`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn get(&self, reminder_slug: &str) -> anyhow::Result<Option<Reminder>> {
        self.reminders.find_by_slug(reminder_slug)
    }

    /// Lists reminders with optional project filter.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn list(
        &self,
        project_id: Option<crate::domain::ProjectId>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Reminder>> {
        self.reminders.list(project_id, include_archived)
    }

    /// Checks for due reminders and marks each one as fired.
    ///
    /// Returns the list of reminders that were fired. Callers are responsible
    /// for sending OS desktop notifications via [`crate::notify::fire`].
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn check_due(&self) -> anyhow::Result<Vec<Reminder>> {
        let due = self.reminders.list_due(Utc::now())?;
        let mut fired = Vec::with_capacity(due.len());
        for r in due {
            let updated = self.reminders.mark_fired(&r.slug)?;
            fired.push(updated);
        }
        Ok(fired)
    }

    /// Updates mutable fields of an existing reminder.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error occurs.
    pub fn update(&self, reminder_slug: &str, patch: ReminderPatch) -> anyhow::Result<Reminder> {
        self.reminders.update(reminder_slug, patch)
    }

    /// Archives a reminder.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error occurs.
    pub fn archive(&self, reminder_slug: &str) -> anyhow::Result<Reminder> {
        self.reminders.archive(reminder_slug)
    }

    /// Restores an archived reminder.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error occurs.
    pub fn restore(&self, reminder_slug: &str) -> anyhow::Result<Reminder> {
        self.reminders.restore(reminder_slug)
    }

    /// Permanently deletes a reminder.
    ///
    /// Only archived reminders may be deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder is not archived, does not exist, or a
    /// database error occurs.
    pub fn delete(&self, reminder_slug: &str) -> anyhow::Result<()> {
        let reminder = self
            .reminders
            .find_by_slug(reminder_slug)?
            .ok_or_else(|| anyhow::anyhow!("reminder '{reminder_slug}' not found"))?;

        if reminder.archived_at.is_none() {
            return Err(anyhow::anyhow!(
                "reminder '{reminder_slug}' must be archived before deletion"
            ));
        }

        self.reminders.delete(reminder_slug)
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn ops() -> ReminderOps {
        let conn = Arc::new(Mutex::new(open_in_memory().expect("in-memory db")));
        ReminderOps::new(conn)
    }

    fn future() -> DateTime<Utc> {
        Utc::now() + chrono::Duration::hours(1)
    }

    #[test]
    fn test_create_reminder() {
        let ops = ops();
        let r = ops
            .create(CreateReminder {
                project_slug: "quick-capture".to_owned(),
                task_slug: None,
                remind_at: future(),
                message: Some("Deploy on Friday".to_owned()),
                persistent: false,
            })
            .expect("create");
        assert!(r.slug.starts_with("quick-capture-reminder-"));
        assert!(!r.fired);
    }

    #[test]
    fn test_create_project_not_found_returns_error() {
        let ops = ops();
        let err = ops
            .create(CreateReminder {
                project_slug: "nonexistent".to_owned(),
                task_slug: None,
                remind_at: future(),
                message: None,
                persistent: false,
            })
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_check_due_fires_past_reminders() {
        let ops = ops();
        // Directly insert a past reminder via the store, bypassing ops validation.
        let past = Utc::now() - chrono::Duration::hours(1);
        ops.reminders
            .create(NewReminder {
                slug: "qc-reminder-past".to_owned(),
                project_id: crate::domain::ProjectId(1),
                task_id: None,
                remind_at: past,
                message: Some("Past".to_owned()),
                persistent: false,
            })
            .expect("create past reminder");

        let fired = ops.check_due().expect("check_due");
        assert_eq!(fired.len(), 1);
        assert!(fired[0].fired);
    }

    #[test]
    fn test_delete_requires_archived() {
        let ops = ops();
        let r = ops
            .create(CreateReminder {
                project_slug: "quick-capture".to_owned(),
                task_slug: None,
                remind_at: future(),
                message: Some("Active".to_owned()),
                persistent: false,
            })
            .expect("create");
        let err = ops.delete(&r.slug).unwrap_err();
        assert!(err.to_string().contains("archived"));
    }

    #[test]
    fn test_update_changes_message() {
        let ops = ops();
        let r = ops
            .create(CreateReminder {
                project_slug: "quick-capture".to_owned(),
                task_slug: None,
                remind_at: future(),
                message: Some("Original".to_owned()),
                persistent: false,
            })
            .expect("create");
        let updated = ops
            .update(
                &r.slug,
                crate::domain::ReminderPatch {
                    remind_at: None,
                    message: Some("Updated".to_owned()),
                    persistent: None,
                },
            )
            .expect("update");
        assert_eq!(updated.message.as_deref(), Some("Updated"));
    }
}
