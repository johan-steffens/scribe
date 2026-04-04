// Rust guideline compliant 2026-02-21
//! Business logic for the time-tracking feature.
//!
//! [`TrackerOps`] wraps `SqliteTimeEntries` and `SqliteProjects` to provide
//! timer start/stop, active timer status, and time-range reports.
//!
//! # Rules
//!
//! - Only one timer may be running at a time. [`TrackerOps::start_timer`]
//!   returns a clear error (with the active slug and elapsed time) if a
//!   timer is already running.
//! - [`TrackerOps::stop_timer`] errors if no timer is currently running.
//! - Reports cover only completed, non-archived entries.
//!
//! # TODO(phase3): migrate to domain error structs per M-ERRORS-CANONICAL-STRUCTS

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

use crate::domain::Projects;
use crate::domain::{
    NewTimeEntry, ProjectId, TaskId, TimeEntries, TimeEntry, TimeEntryPatch, slug,
};
use crate::store::{SqliteProjects, SqliteTimeEntries};

/// Parameters for starting a new timer via [`TrackerOps`].
#[derive(Debug, Clone)]
pub struct StartTimer {
    /// Slug of the owning project (used for slug prefix generation).
    pub project_slug: String,
    /// Numeric ID of the owning project.
    pub project_id: ProjectId,
    /// Optional linked task ID.
    pub task_id: Option<TaskId>,
    /// Optional free-text note.
    pub note: Option<String>,
}

/// High-level timer operations with conflict detection.
///
/// Construct via [`TrackerOps::new`], passing the shared database connection.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::ops::TrackerOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let ops = TrackerOps::new(conn);
/// ```
#[derive(Clone, Debug)]
pub struct TrackerOps {
    entries: SqliteTimeEntries,
    projects: SqliteProjects,
}

impl TrackerOps {
    /// Creates a new [`TrackerOps`] backed by the given connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::ops::TrackerOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let ops = TrackerOps::new(conn);
    /// ```
    #[must_use]
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            entries: SqliteTimeEntries::new(Arc::clone(&conn)),
            projects: SqliteProjects::new(conn),
        }
    }

    /// Starts a new timer for the given project.
    ///
    /// Returns an error with the active entry slug and elapsed duration if a
    /// timer is already running.
    ///
    /// # Errors
    ///
    /// Returns an error if a timer is already running, the project is not
    /// found, slug generation fails, or a database error occurs.
    pub fn start_timer(&self, params: StartTimer) -> anyhow::Result<TimeEntry> {
        // Enforce single-active-timer invariant.
        if let Some(running) = self.entries.find_running()? {
            let elapsed = Utc::now() - running.started_at;
            let mins = elapsed.num_minutes();
            let secs = elapsed.num_seconds() % 60;
            return Err(anyhow::anyhow!(
                "timer '{}' is already running ({mins}m {secs}s elapsed); \
                 stop it first with `scribe track stop`",
                running.slug,
            ));
        }

        let now = Utc::now();
        // Slug format: {project_slug}-entry-{YYYYMMDD}-{HHmmss}
        let date_part = now.format("%Y%m%d").to_string();
        let time_part = now.format("%H%M%S").to_string();
        let base_slug = format!("{}-entry-{date_part}-{time_part}", params.project_slug);
        let unique_slug = slug::ensure_unique(&base_slug, |candidate| {
            self.entries
                .find_by_slug(candidate)
                .map(|r| r.is_some())
                .unwrap_or(false)
        })
        .map_err(|e| anyhow::anyhow!("slug generation failed: {e}"))?;

        self.entries.create(NewTimeEntry {
            slug: unique_slug,
            project_id: params.project_id,
            task_id: params.task_id,
            started_at: now,
            note: params.note,
        })
    }

    /// Stops the currently running timer.
    ///
    /// Returns an error if no timer is running.
    ///
    /// # Errors
    ///
    /// Returns an error if no timer is running or a database error occurs.
    pub fn stop_timer(&self) -> anyhow::Result<TimeEntry> {
        let running = self
            .entries
            .find_running()?
            .ok_or_else(|| anyhow::anyhow!("no timer is currently running"))?;

        self.entries.stop(&running.slug, Utc::now())
    }

    /// Returns the currently active timer and its elapsed duration.
    ///
    /// Returns `None` when no timer is running.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn timer_status(&self) -> anyhow::Result<Option<(TimeEntry, Duration)>> {
        let Some(entry) = self.entries.find_running()? else {
            return Ok(None);
        };
        let elapsed = Utc::now() - entry.started_at;
        Ok(Some((entry, elapsed)))
    }

    /// Returns completed time entries within `[since, until)` for optional project filter.
    ///
    /// Entries are returned with their computed durations. Only completed
    /// (non-running), non-archived entries are included.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn report(
        &self,
        project_id: Option<ProjectId>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> anyhow::Result<Vec<(TimeEntry, Duration)>> {
        let entries = self
            .entries
            .list_completed_in_range(project_id, since, until)?;

        let result = entries
            .into_iter()
            .filter_map(|e| {
                let ended = e.ended_at?;
                let dur = ended - e.started_at;
                Some((e, dur))
            })
            .collect();

        Ok(result)
    }

    /// Resolves a project slug to a [`ProjectId`], erroring if not found or archived.
    ///
    /// A convenience used by the CLI layer before calling [`start_timer`].
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist, is archived, or a
    /// database error occurs.
    ///
    /// [`start_timer`]: Self::start_timer
    pub fn resolve_project(&self, project_slug: &str) -> anyhow::Result<(String, ProjectId)> {
        let project = self
            .projects
            .find_by_slug(project_slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;

        if project.archived_at.is_some() {
            return Err(anyhow::anyhow!(
                "project '{project_slug}' is archived; restore it first"
            ));
        }

        Ok((project.slug, project.id))
    }

    /// Lists recent time entries (most-recent-first, up to `limit`).
    ///
    /// Only non-archived entries are returned.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn list_recent(&self, limit: usize) -> anyhow::Result<Vec<TimeEntry>> {
        let all = self.entries.list(None, false)?;
        Ok(all.into_iter().take(limit).collect())
    }

    /// Updates the note on an existing time entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry does not exist or a database error occurs.
    pub fn update_note(&self, entry_slug: &str, note: Option<String>) -> anyhow::Result<TimeEntry> {
        self.entries.update(entry_slug, TimeEntryPatch { note })
    }

    /// Archives a time entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry does not exist or a database error occurs.
    pub fn archive_entry(&self, entry_slug: &str) -> anyhow::Result<TimeEntry> {
        self.entries.archive(entry_slug)
    }
}

// ── test helpers ─────────────────────────────────────────────────────────

#[cfg(test)]
pub mod testing {
    //! Test helpers for the tracker ops module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::TrackerOps`] instances against an in-memory database.

    use super::*;
    use crate::db::open_in_memory;

    /// Constructs a [`TrackerOps`] backed by an in-memory database.
    #[must_use]
    pub fn ops() -> TrackerOps {
        let conn = Arc::new(Mutex::new(open_in_memory().expect("in-memory db")));
        TrackerOps::new(conn)
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn ops() -> TrackerOps {
        let conn = Arc::new(Mutex::new(open_in_memory().expect("in-memory db")));
        TrackerOps::new(conn)
    }

    #[test]
    fn test_start_timer_creates_entry() {
        let ops = ops();
        let entry = ops
            .start_timer(StartTimer {
                project_slug: "quick-capture".to_owned(),
                project_id: ProjectId(1),
                task_id: None,
                note: None,
            })
            .expect("start");
        assert!(entry.ended_at.is_none());
        assert!(entry.slug.starts_with("quick-capture-entry-"));
    }

    #[test]
    fn test_start_timer_blocked_when_running() {
        let ops = ops();
        ops.start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .expect("first start");

        let err = ops
            .start_timer(StartTimer {
                project_slug: "quick-capture".to_owned(),
                project_id: ProjectId(1),
                task_id: None,
                note: None,
            })
            .unwrap_err();
        assert!(err.to_string().contains("already running"));
    }

    #[test]
    fn test_stop_timer() {
        let ops = ops();
        ops.start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .expect("start");
        let stopped = ops.stop_timer().expect("stop");
        assert!(stopped.ended_at.is_some());
    }

    #[test]
    fn test_stop_timer_when_none_running() {
        let ops = ops();
        let err = ops.stop_timer().unwrap_err();
        assert!(err.to_string().contains("no timer"));
    }

    #[test]
    fn test_timer_status_none_when_idle() {
        let ops = ops();
        assert!(ops.timer_status().expect("status").is_none());
    }

    #[test]
    fn test_timer_status_returns_elapsed() {
        let ops = ops();
        ops.start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .expect("start");
        let (_, elapsed) = ops.timer_status().expect("status").expect("running");
        assert!(elapsed.num_seconds() >= 0);
    }

    #[test]
    fn test_list_recent_returns_entries() {
        let ops = ops();
        ops.start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .expect("start");
        ops.stop_timer().expect("stop");
        let recent = ops.list_recent(10).expect("list_recent");
        assert!(!recent.is_empty());
    }

    #[test]
    fn test_update_note_changes_note() {
        let ops = ops();
        let entry = ops
            .start_timer(StartTimer {
                project_slug: "quick-capture".to_owned(),
                project_id: ProjectId(1),
                task_id: None,
                note: None,
            })
            .expect("start");
        let updated = ops
            .update_note(&entry.slug, Some("My note".to_owned()))
            .expect("update note");
        assert_eq!(updated.note.as_deref(), Some("My note"));
    }

    #[test]
    fn test_archive_entry_archives() {
        let ops = ops();
        let entry = ops
            .start_timer(StartTimer {
                project_slug: "quick-capture".to_owned(),
                project_id: ProjectId(1),
                task_id: None,
                note: None,
            })
            .expect("start");
        ops.stop_timer().expect("stop");
        let archived = ops.archive_entry(&entry.slug).expect("archive entry");
        assert!(archived.archived_at.is_some());
    }
}
