// Rust guideline compliant 2026-02-21
//! `Reminder` entity and the `Reminders` repository trait.
//!
//! Reminders are scheduled notifications linked to a project and optionally a
//! task. Slugs are auto-generated from project and title,
//! e.g. `payments-reminder-deploy-friday`.
//!
//! # Phase 2 additions
//!
//! [`Reminders::list_due`] and [`Reminders::mark_fired`] are added here to
//! support the `ops::reminders::check_due` workflow.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{ProjectId, ReminderId, TaskId};

// ── entity struct ──────────────────────────────────────────────────────────

/// A reminder record as stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reminder {
    /// Internal numeric primary key (not exposed to users).
    pub id: ReminderId,
    /// Unique slug, e.g. `payments-reminder-deploy-friday`.
    pub slug: String,
    /// The project this reminder belongs to.
    pub project_id: ProjectId,
    /// The slug of the project this reminder belongs to (used for sync).
    pub project_slug: String,
    /// Optional linked task.
    pub task_id: Option<TaskId>,
    /// The slug of the linked task (used for sync).
    pub task_slug: Option<String>,
    /// When the reminder should fire.
    pub remind_at: DateTime<Utc>,
    /// Optional free-text message to display.
    pub message: Option<String>,
    /// Whether the reminder has already fired.
    pub fired: bool,
    /// When `true`, the notification blocks until the user dismisses it.
    ///
    /// On macOS this uses `display alert` (modal) instead of
    /// `display notification` (banner). On other platforms `notify-rust`
    /// is used regardless — persistent has no effect there yet.
    pub persistent: bool,
    /// Timestamp when archived; `None` means the reminder is active.
    pub archived_at: Option<DateTime<Utc>>,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
}

// ── repository trait ───────────────────────────────────────────────────────

/// Data-access operations for the `reminders` table.
// TODO(phase3): migrate to domain error structs per M-ERRORS-CANONICAL-STRUCTS
pub trait Reminders {
    /// Inserts a new reminder and returns the persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug already exists or a database error occurs.
    fn create(&self, reminder: NewReminder) -> anyhow::Result<Reminder>;

    /// Looks up a reminder by its slug.
    ///
    /// Returns `Ok(None)` when no reminder with that slug exists.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Reminder>>;

    /// Lists reminders, with optional filtering.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list(
        &self,
        project_id: Option<ProjectId>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Reminder>>;

    /// Archives the reminder identified by `slug`.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error
    /// occurs.
    fn archive(&self, slug: &str) -> anyhow::Result<Reminder>;

    /// Restores an archived reminder.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error
    /// occurs.
    fn restore(&self, slug: &str) -> anyhow::Result<Reminder>;

    /// Permanently deletes the reminder row.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error
    /// occurs.
    fn delete(&self, slug: &str) -> anyhow::Result<()>;

    /// Returns all unfired, non-archived reminders with `remind_at <= before`.
    ///
    /// Used by [`crate::ops::reminders::ReminderOps::check_due`] to surface
    /// notifications on startup.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list_due(&self, before: DateTime<Utc>) -> anyhow::Result<Vec<Reminder>>;

    /// Marks the reminder as fired by setting `fired = 1`.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error
    /// occurs.
    fn mark_fired(&self, slug: &str) -> anyhow::Result<Reminder>;

    /// Archives all reminders belonging to the given project.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()>;

    /// Updates mutable fields of an existing reminder.
    ///
    /// # Errors
    ///
    /// Returns an error if the reminder does not exist or a database error occurs.
    fn update(&self, slug: &str, patch: ReminderPatch) -> anyhow::Result<Reminder>;
}

// ── input types ────────────────────────────────────────────────────────────

/// Parameters required to create a new reminder.
#[derive(Debug, Clone)]
pub struct NewReminder {
    /// Pre-generated unique slug.
    pub slug: String,
    /// Owning project.
    pub project_id: ProjectId,
    /// Optional linked task.
    pub task_id: Option<TaskId>,
    /// When the reminder should fire.
    pub remind_at: DateTime<Utc>,
    /// Optional message text.
    pub message: Option<String>,
    /// Whether the notification should block until dismissed.
    pub persistent: bool,
}

/// Partial update for mutable reminder fields.
#[derive(Debug, Clone, Default)]
pub struct ReminderPatch {
    /// New fire time, if changing.
    pub remind_at: Option<DateTime<Utc>>,
    /// New message text, if changing.
    pub message: Option<String>,
    /// Change the persistent flag, if desired.
    pub persistent: Option<bool>,
}
