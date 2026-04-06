//! Business logic operations for cross-domain reporting.
//!
//! [`ReportingOps`] aggregates data from all stores to produce summary and
//! per-project reports with flexible time filtering.

use std::sync::{Arc, Mutex};

use chrono::Duration;
use rusqlite::Connection;

use crate::domain::{Project, Task, TimeEntry, Todo};

/// Aggregated summary report across all domains.
#[derive(Debug, Clone)]
pub struct SummaryReport {
    /// Number of active (non-archived) projects.
    pub active_projects: usize,
    /// Number of pending tasks (status != Done && status != Cancelled).
    pub pending_tasks: usize,
    /// Number of open todos (not done).
    pub open_todos: usize,
    /// Number of unprocessed inbox items.
    pub items_in_inbox: usize,
    /// Number of active reminders.
    pub active_reminders: usize,
    /// Total time tracked across all entries in the reporting window.
    pub total_time_tracked: Duration,
    /// Number of tasks with due dates in the past.
    pub overdue_tasks: usize,
}

/// Per-project report with task, todo, and time breakdown.
#[derive(Debug, Clone)]
pub struct ProjectReport {
    /// The project entity.
    pub project: Project,
    /// Pending tasks in this project.
    pub pending_tasks: Vec<Task>,
    /// Open todos in this project.
    pub open_todos: Vec<Todo>,
    /// Time entries with their computed durations.
    pub time_entries: Vec<(TimeEntry, Duration)>,
    /// Sum of all time entry durations in this project.
    pub total_time: Duration,
    /// Task completion percentage (0.0 to 100.0).
    pub completion_percentage: f32,
}

/// Centralized reporting operations aggregating all domain stores.
///
/// Construct via [`ReportingOps::new`], passing the shared database connection.
#[derive(Clone, Debug)]
pub struct ReportingOps;

impl ReportingOps {
    /// Creates a new [`ReportingOps`] backed by the given connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::ops::ReportingOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let ops = ReportingOps::new(conn);
    /// ```
    #[must_use]
    pub fn new(_conn: Arc<Mutex<Connection>>) -> Self {
        Self
    }
}
