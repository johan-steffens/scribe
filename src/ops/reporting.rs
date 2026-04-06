//! Business logic operations for cross-domain reporting.
//!
//! [`ReportingOps`] aggregates data from all stores to produce summary and
//! per-project reports with flexible time filtering.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

use crate::domain::capture::CaptureItems;
use crate::domain::project::Projects;
use crate::domain::reminder::Reminders;
use crate::domain::task::Tasks;
use crate::domain::time_entry::TimeEntries;
use crate::domain::todo::Todos;
use crate::domain::{Project, ProjectId, Task, TaskStatus, TimeEntry, Todo};
use crate::store::{
    SqliteCaptureItems, SqliteProjects, SqliteReminders, SqliteTasks, SqliteTimeEntries,
    SqliteTodos,
};

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

/// Detailed task report covering lifecycle and time tracking.
#[derive(Debug, Clone)]
pub struct TaskReport {
    /// The task entity.
    pub task: Task,
    /// Time entries linked to this task.
    pub time_entries: Vec<(TimeEntry, Duration)>,
    /// Total time tracked on this task.
    pub total_time: Duration,
}

/// Centralized reporting operations aggregating all domain stores.
///
/// Construct via [`ReportingOps::new`], passing the shared database connection.
#[derive(Clone, Debug)]
pub struct ReportingOps {
    projects: SqliteProjects,
    tasks: SqliteTasks,
    todos: SqliteTodos,
    entries: SqliteTimeEntries,
    reminders: SqliteReminders,
    inbox: SqliteCaptureItems,
}

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
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            projects: SqliteProjects::new(Arc::clone(&conn)),
            tasks: SqliteTasks::new(Arc::clone(&conn)),
            todos: SqliteTodos::new(Arc::clone(&conn)),
            entries: SqliteTimeEntries::new(Arc::clone(&conn)),
            reminders: SqliteReminders::new(Arc::clone(&conn)),
            inbox: SqliteCaptureItems::new(conn),
        }
    }

    /// Generates a summary report across all domains for the given time window.
    ///
    /// # Errors
    ///
    /// Returns an error if any database query fails.
    pub fn summary_report(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> anyhow::Result<SummaryReport> {
        // Count active projects (non-archived)
        let active_projects = self.projects.list_active()?.len();

        // List all non-archived tasks and filter pending
        let all_tasks = self.tasks.list(None, None, None, false)?;
        let pending_tasks = all_tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Done && t.status != TaskStatus::Cancelled)
            .count();

        // Count open (non-done, non-archived) todos
        let open_todos = self.todos.list(None, false, false)?.len();

        // Count unprocessed inbox items
        let items_in_inbox = self.inbox.list(false)?.len();

        // Count active (non-archived) reminders
        let active_reminders = self.reminders.list(None, false)?.len();

        // Sum time tracked in the window
        let time_entries = self.entries.list_completed_in_range(None, since, until)?;
        let total_time_tracked = compute_total_time(&time_entries);

        // Count overdue tasks (pending tasks with due_date in the past)
        let today = Utc::now().date_naive();
        let overdue_tasks = all_tasks
            .iter()
            .filter(|t| {
                t.status != TaskStatus::Done
                    && t.status != TaskStatus::Cancelled
                    && t.due_date.is_some_and(|d| d < today)
            })
            .count();

        Ok(SummaryReport {
            active_projects,
            pending_tasks,
            open_todos,
            items_in_inbox,
            active_reminders,
            total_time_tracked,
            overdue_tasks,
        })
    }

    /// Generates a detailed report for a specific project.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is not found or any database query fails.
    pub fn project_report(
        &self,
        slug: &str,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> anyhow::Result<ProjectReport> {
        let project = self
            .projects
            .find_by_slug(slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{slug}' not found"))?;

        // Get pending tasks for this project
        let pending_tasks = self
            .tasks
            .list(Some(project.id), None, None, false)?
            .into_iter()
            .filter(|t| t.status != TaskStatus::Done && t.status != TaskStatus::Cancelled)
            .collect();

        // Get open todos for this project
        let open_todos = self
            .todos
            .list(Some(project.id), false, false)?
            .into_iter()
            .filter(|t| !t.done)
            .collect();

        // Get time entries in range for this project
        let time_entries = self
            .entries
            .list_completed_in_range(Some(project.id), since, until)?;

        let time_entries_with_duration: Vec<_> = time_entries
            .iter()
            .map(|e| {
                let duration = compute_entry_duration(e);
                (e.clone(), duration)
            })
            .collect();

        let total_time = time_entries_with_duration
            .iter()
            .map(|(_, d)| *d)
            .fold(Duration::zero(), |acc, d| acc + d);

        // Calculate completion percentage
        let all_tasks = self.tasks.list(Some(project.id), None, None, false)?;
        let total_tasks = all_tasks.len();
        let done_tasks = all_tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .count();
        let completion_percentage = if total_tasks > 0 {
            // Conversion from usize to f32 is safe for task counts
            #[allow(
                clippy::cast_precision_loss,
                reason = "f32 precision is sufficient for percentages up to 100"
            )]
            let percentage = (done_tasks as f32 / total_tasks as f32) * 100.0;
            percentage
        } else {
            0.0
        };

        Ok(ProjectReport {
            project,
            pending_tasks,
            open_todos,
            time_entries: time_entries_with_duration,
            total_time,
            completion_percentage,
        })
    }

    /// Returns completed time entries within `[since, until)` for optional project filter.
    ///
    /// Entries are returned with their computed durations. Only completed
    /// (non-running), non-archived entries are included.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn time_report(
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

    /// Generates a detailed report for a specific task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is not found or any database query fails.
    pub fn task_report(&self, slug: &str) -> anyhow::Result<TaskReport> {
        let task = self
            .tasks
            .find_by_slug(slug)?
            .ok_or_else(|| anyhow::anyhow!("task '{slug}' not found"))?;

        // Get time entries linked to this task
        let all_entries = self.entries.list(Some(task.project_id), false)?;
        let task_entries: Vec<TimeEntry> = all_entries
            .into_iter()
            .filter(|e| e.task_id == Some(task.id))
            .collect();

        let time_entries_with_duration: Vec<_> = task_entries
            .iter()
            .map(|e| {
                let duration = compute_entry_duration(e);
                (e.clone(), duration)
            })
            .collect();

        let total_time = time_entries_with_duration
            .iter()
            .map(|(_, d)| *d)
            .fold(Duration::zero(), |acc, d| acc + d);

        Ok(TaskReport {
            task,
            time_entries: time_entries_with_duration,
            total_time,
        })
    }
}

// ── internal helpers ────────────────────────────────────────────────────────

/// Computes the duration of a single time entry.
fn compute_entry_duration(entry: &TimeEntry) -> Duration {
    match entry.ended_at {
        Some(ended) => ended.signed_duration_since(entry.started_at),
        None => Utc::now().signed_duration_since(entry.started_at),
    }
}

/// Sums up total duration from a collection of time entries.
fn compute_total_time(entries: &[TimeEntry]) -> Duration {
    entries
        .iter()
        .map(compute_entry_duration)
        .fold(Duration::zero(), |acc, d| acc + d)
}
