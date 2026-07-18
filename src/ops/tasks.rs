//! Business logic operations for the task entity.
//!
//! [`TaskOps`] wraps `SqliteTasks` and adds slug generation on task creation.
//! The slug is derived from the project slug and task title, and is made
//! unique via [`crate::domain::slug::ensure_unique`].

use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::domain::{
    NewTask, ProjectId, Projects, Task, TaskId, TaskPatch, TaskPriority, TaskStatus, Tasks, slug,
};
use crate::store::{SqliteProjects, SqliteTasks};

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
    /// Optional parent task ID for hierarchical nesting.
    pub parent_id: Option<TaskId>,
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
    projects: SqliteProjects,
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
            tasks: SqliteTasks::new(Arc::clone(&conn)),
            projects: SqliteProjects::new(conn),
        }
    }

    /// Creates a new task, auto-generating a unique slug from the title.
    ///
    /// The slug format is `{project_slug}-task-{title-slug}`, with a random
    /// 4-character suffix appended on collision.
    ///
    /// Validates that the title is non-empty and that the owning project exists
    /// and is not archived (mirrors [`crate::ops::TodoOps::create`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the title is empty, the project is missing or
    /// archived, slug generation fails after all retries, or a database error
    /// occurs.
    pub fn create_task(&self, params: CreateTask) -> anyhow::Result<Task> {
        let title = params.title.trim();
        if title.is_empty() {
            return Err(anyhow::anyhow!("task title cannot be empty"));
        }

        let project = self
            .projects
            .find_by_slug(&params.project_slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{}' not found", params.project_slug))?;

        if project.archived_at.is_some() {
            return Err(anyhow::anyhow!(
                "project '{}' is archived; restore it before adding tasks",
                params.project_slug
            ));
        }

        // Prefer the live project id so callers cannot attach to a stale id.
        // `params.project_id` is retained on `CreateTask` for call-site
        // convenience but is not trusted.
        let _ = params.project_id;
        let project_id = project.id;
        let project_slug = project.slug;

        let prefix = format!("{project_slug}-task-");
        let base_slug = slug::generate(&prefix, title);
        let unique_slug = slug::ensure_unique(&base_slug, |candidate| {
            self.tasks
                .find_by_slug(candidate)
                .is_ok_and(|r| r.is_some())
        })
        .map_err(|e| anyhow::anyhow!("slug generation failed: {e}"))?;

        self.tasks.create(NewTask {
            slug: unique_slug,
            project_id,
            title: title.to_owned(),
            description: params.description,
            status: params.status,
            priority: params.priority,
            due_date: params.due_date,
            parent_id: params.parent_id,
            kind: crate::domain::TaskKind::Task,
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
