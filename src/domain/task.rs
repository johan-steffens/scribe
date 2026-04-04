//! `Task` entity, status/priority enums, and the `Tasks` repository trait.
//!
//! Tasks are the primary unit of work. They belong to a project and carry a
//! status, priority, and optional due date. Slugs are auto-generated from the
//! title, e.g. `payments-task-fix-login`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{ProjectId, TaskId};

// ── status enum ────────────────────────────────────────────────────────────

/// Lifecycle status of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task has not been started.
    Todo,
    /// Task is actively being worked on.
    InProgress,
    /// Task has been completed.
    Done,
    /// Task was cancelled and will not be completed.
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Todo => write!(f, "todo"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Done => write!(f, "done"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "todo" => Ok(Self::Todo),
            "in_progress" => Ok(Self::InProgress),
            "done" => Ok(Self::Done),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("unknown task status '{other}'")),
        }
    }
}

// ── priority enum ──────────────────────────────────────────────────────────

/// Urgency level of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    /// Nice-to-have, no deadline pressure.
    Low,
    /// Default priority level.
    Medium,
    /// Should be done soon.
    High,
    /// Needs immediate attention.
    Urgent,
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Urgent => write!(f, "urgent"),
        }
    }
}

impl std::str::FromStr for TaskPriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "urgent" => Ok(Self::Urgent),
            other => Err(format!("unknown task priority '{other}'")),
        }
    }
}

// ── entity struct ──────────────────────────────────────────────────────────

/// A task record as stored in the database.
///
/// Tasks belong to a project (`project_id`) and have an auto-generated slug
/// derived from the project slug and title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// Internal numeric primary key (not exposed to users).
    pub id: TaskId,
    /// Unique slug, e.g. `payments-task-fix-login`.
    pub slug: String,
    /// The project this task belongs to.
    pub project_id: ProjectId,
    /// The slug of the project this task belongs to (used for sync).
    pub project_slug: String,
    /// Short task title.
    pub title: String,
    /// Optional detailed description.
    pub description: Option<String>,
    /// Current lifecycle status.
    pub status: TaskStatus,
    /// Urgency level.
    pub priority: TaskPriority,
    /// Optional due date (date only, no time component).
    pub due_date: Option<NaiveDate>,
    /// Timestamp when archived; `None` means the task is active.
    pub archived_at: Option<DateTime<Utc>>,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Last-updated timestamp (UTC).
    pub updated_at: DateTime<Utc>,
}

// ── repository trait ───────────────────────────────────────────────────────

/// Data-access operations for the `tasks` table.
pub trait Tasks {
    /// Inserts a new task and returns the persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug already exists or a database error occurs.
    fn create(&self, task: NewTask) -> anyhow::Result<Task>;

    /// Looks up a task by its slug.
    ///
    /// Returns `Ok(None)` when no task with that slug exists.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Task>>;

    /// Lists tasks, with optional filtering.
    ///
    /// When `project_id` is `Some`, only tasks for that project are returned.
    /// When `include_archived` is `true`, archived tasks are included.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list(
        &self,
        project_id: Option<ProjectId>,
        status: Option<TaskStatus>,
        priority: Option<TaskPriority>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Task>>;

    /// Updates mutable fields of an existing task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    fn update(&self, slug: &str, patch: TaskPatch) -> anyhow::Result<Task>;

    /// Archives the task identified by `slug`.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    fn archive(&self, slug: &str) -> anyhow::Result<Task>;

    /// Restores an archived task, clearing its `archived_at` timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    fn restore(&self, slug: &str) -> anyhow::Result<Task>;

    /// Permanently deletes the task row from the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the task does not exist or a database error occurs.
    fn delete(&self, slug: &str) -> anyhow::Result<()>;

    /// Archives all tasks belonging to the given project.
    ///
    /// Used when a project is archived to cascade the operation.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()>;
}

// ── input types ────────────────────────────────────────────────────────────

/// Parameters required to create a new task.
#[derive(Debug, Clone)]
pub struct NewTask {
    /// Pre-generated unique slug.
    pub slug: String,
    /// Owning project.
    pub project_id: ProjectId,
    /// Short task title.
    pub title: String,
    /// Optional detailed description.
    pub description: Option<String>,
    /// Initial status.
    pub status: TaskStatus,
    /// Urgency level.
    pub priority: TaskPriority,
    /// Optional due date.
    pub due_date: Option<NaiveDate>,
}

/// Partial update for mutable task fields.
///
/// `None` values are not written.
#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    /// New title, if changing.
    pub title: Option<String>,
    /// New description, if changing.
    pub description: Option<String>,
    /// Whether to clear the description.
    pub clear_description: bool,
    /// New status, if changing.
    pub status: Option<TaskStatus>,
    /// New priority, if changing.
    pub priority: Option<TaskPriority>,
    /// New due date, if changing.
    pub due_date: Option<NaiveDate>,
    /// Whether to clear the due date.
    pub clear_due_date: bool,
    /// New project assignment, if moving to a different project.
    pub project_id: Option<ProjectId>,
}
