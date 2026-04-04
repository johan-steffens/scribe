// Rust guideline compliant 2026-02-21
//! Business logic operations for the task entity.
//!
//! [`TaskOps`] wraps `SqliteTasks` and adds slug generation on task creation.
//! The slug is derived from the project slug and task title, and is made
//! unique via [`crate::domain::slug::ensure_unique`].

use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::domain::{NewTask, ProjectId, Task, TaskPatch, TaskPriority, TaskStatus, Tasks, slug};
use crate::store::SqliteTasks;

/// Parameters for creating a new task via [`TaskOps`].
///
/// The `slug` field is auto-generated from `project_slug` and `title`.
#[derive(Debug, Clone)]
pub struct CreateTask {
    /// Slug of the owning project (used for slug prefix generation).
    pub project_slug: String,
    /// Numeric ID of the owning project.
    pub project_id: ProjectId,
    /// Task title.
    pub title: String,
    /// Optional detailed description.
    pub description: Option<String>,
    /// Initial status (defaults to `Todo`).
    pub status: TaskStatus,
    /// Urgency level.
    pub priority: TaskPriority,
    /// Optional due date.
    pub due_date: Option<NaiveDate>,
}

/// High-level task operations with slug generation on create.
///
/// Construct via [`TaskOps::new`], passing the shared database connection.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::ops::TaskOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let ops = TaskOps::new(conn);
/// ```
#[derive(Clone, Debug)]
pub struct TaskOps {
    tasks: SqliteTasks,
}

impl TaskOps {
    /// Creates a new [`TaskOps`] backed by the given connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::ops::TaskOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let ops = TaskOps::new(conn);
    /// ```
    #[must_use]
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            tasks: SqliteTasks::new(conn),
        }
    }

    /// Creates a new task, auto-generating a unique slug from the title.
    ///
    /// The slug format is `{project_slug}-task-{title-slug}`, with a random
    /// 4-character suffix appended on collision.
    ///
    /// # Errors
    ///
    /// Returns an error if slug generation fails after all retries, or if a
    /// database error occurs.
    pub fn create_task(&self, params: CreateTask) -> anyhow::Result<Task> {
        let prefix = format!("{}-task-", params.project_slug);
        let base_slug = slug::generate(&prefix, &params.title);
        let unique_slug = slug::ensure_unique(&base_slug, |candidate| {
            self.tasks
                .find_by_slug(candidate)
                .map(|r| r.is_some())
                .unwrap_or(false)
        })
        .map_err(|e| anyhow::anyhow!("slug generation failed: {e}"))?;

        self.tasks.create(NewTask {
            slug: unique_slug,
            project_id: params.project_id,
            title: params.title,
            description: params.description,
            status: params.status,
            priority: params.priority,
            due_date: params.due_date,
        })
    }

    /// Returns the task with the given `slug`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn get_task(&self, slug: &str) -> anyhow::Result<Option<Task>> {
        self.tasks.find_by_slug(slug)
    }

    /// Lists tasks with optional filtering.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn list_tasks(
        &self,
        project_id: Option<ProjectId>,
        status: Option<TaskStatus>,
        priority: Option<TaskPriority>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Task>> {
        self.tasks
            .list(project_id, status, priority, include_archived)
    }

    /// Updates mutable fields of an existing task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    pub fn update_task(&self, slug: &str, patch: TaskPatch) -> anyhow::Result<Task> {
        self.tasks.update(slug, patch)
    }

    /// Marks a task as done by setting its status to `Done`.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    pub fn mark_done(&self, slug: &str) -> anyhow::Result<Task> {
        self.tasks.update(
            slug,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
    }

    /// Archives a task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    pub fn archive_task(&self, slug: &str) -> anyhow::Result<Task> {
        self.tasks.archive(slug)
    }

    /// Restores an archived task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    pub fn restore_task(&self, slug: &str) -> anyhow::Result<Task> {
        self.tasks.restore(slug)
    }

    /// Deletes a task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    pub fn delete_task(&self, slug: &str) -> anyhow::Result<()> {
        self.tasks.delete(slug)
    }
}

// ── test helpers ─────────────────────────────────────────────────────────

#[cfg(feature = "test-util")]
pub mod testing {
    //! Test helpers for the task ops module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::TaskOps`] instances against an in-memory database.

    use super::{Arc, Mutex, TaskOps};
    use crate::db::open_in_memory;

    /// Constructs a [`TaskOps`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn ops() -> TaskOps {
        let conn = open_in_memory().expect("in-memory db");
        TaskOps::new(Arc::new(Mutex::new(conn)))
    }
}
