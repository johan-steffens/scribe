// Rust guideline compliant 2026-02-21
//! `TimeEntry` entity and the `TimeEntries` repository trait.
//!
//! Time entries record started/stopped timer sessions linked to a project and
//! optionally a task. A running timer has `ended_at = None`.
//! Slugs are auto-generated from the project and start time,
//! e.g. `payments-entry-20260331-143000`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{ProjectId, TaskId, TimeEntryId};

// ── entity struct ──────────────────────────────────────────────────────────

/// A time-entry record as stored in the database.
///
/// `ended_at` is `None` while the timer is still running.
// Phase 2+: not yet constructed in production code paths.
#[allow(dead_code, reason = "used in Phase 2 time tracking feature")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeEntry {
    /// Internal numeric primary key (not exposed to users).
    pub id: TimeEntryId,
    /// Unique slug, e.g. `payments-entry-20260331-143000`.
    pub slug: String,
    /// The project this entry belongs to.
    pub project_id: ProjectId,
    /// Optional linked task.
    pub task_id: Option<TaskId>,
    /// When the timer was started.
    pub started_at: DateTime<Utc>,
    /// When the timer was stopped; `None` means it is still running.
    pub ended_at: Option<DateTime<Utc>>,
    /// Optional free-text note.
    pub note: Option<String>,
    /// Timestamp when archived; `None` means the entry is active.
    pub archived_at: Option<DateTime<Utc>>,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
}

// ── repository trait ───────────────────────────────────────────────────────

/// Data-access operations for the `time_entries` table.
// Phase 2+: not yet used in production paths.
#[allow(dead_code, reason = "used in Phase 2 time tracking feature")]
pub trait TimeEntries {
    /// Inserts a new time entry and returns the persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug already exists or a database error occurs.
    fn create(&self, entry: NewTimeEntry) -> anyhow::Result<TimeEntry>;

    /// Looks up a time entry by its slug.
    ///
    /// Returns `Ok(None)` when no entry with that slug exists.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<TimeEntry>>;

    /// Returns the currently running time entry (if any).
    ///
    /// At most one entry can have `ended_at = NULL` at any given time.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_running(&self) -> anyhow::Result<Option<TimeEntry>>;

    /// Lists time entries, with optional filtering.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list(
        &self,
        project_id: Option<ProjectId>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<TimeEntry>>;

    /// Stops a running timer by setting `ended_at` to now.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry does not exist or a database error occurs.
    fn stop(&self, slug: &str, ended_at: DateTime<Utc>) -> anyhow::Result<TimeEntry>;

    /// Archives the entry identified by `slug`.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry does not exist or a database error occurs.
    fn archive(&self, slug: &str) -> anyhow::Result<TimeEntry>;

    /// Restores an archived entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry does not exist or a database error occurs.
    fn restore(&self, slug: &str) -> anyhow::Result<TimeEntry>;

    /// Archives all time entries belonging to the given project.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()>;
}

// ── input types ────────────────────────────────────────────────────────────

/// Parameters required to create a new time entry.
// Phase 2+: not yet constructed in production code paths.
#[allow(dead_code, reason = "used in Phase 2 time tracking feature")]
#[derive(Debug, Clone)]
pub struct NewTimeEntry {
    /// Pre-generated unique slug.
    pub slug: String,
    /// Owning project.
    pub project_id: ProjectId,
    /// Optional linked task.
    pub task_id: Option<TaskId>,
    /// Timer start time.
    pub started_at: DateTime<Utc>,
    /// Optional note.
    pub note: Option<String>,
}
