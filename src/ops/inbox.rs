// Rust guideline compliant 2026-02-21
//! Business logic for the quick-capture inbox feature.
//!
//! [`InboxOps`] wraps [`SqliteCaptureItems`] and coordinates with the task
//! and todo stores to process inbox items.
//!
//! # Workflow
//!
//! 1. The user captures a thought with `scribe capture <text>`.
//! 2. Later, the user runs `scribe inbox process <slug>` and picks an action.
//! 3. The [`ProcessAction`] enum drives the correct creation call and marks
//!    the capture item as processed.
//!
//! # TODO(phase3): migrate to domain error structs per M-ERRORS-CANONICAL-STRUCTS

use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::Connection;

use crate::domain::TaskStatus;
use crate::domain::{
    slug, CaptureItem, CaptureItems, NewCaptureItem, NewTodo, Projects, TaskPriority, Todos,
};
use crate::ops::tasks::CreateTask;
use crate::ops::TaskOps;
use crate::store::{SqliteCaptureItems, SqliteProjects, SqliteTodos};

/// Action to take when processing a capture item.
///
/// Each variant performs the appropriate operation and marks the capture item
/// as processed atomically.
#[derive(Debug, Clone)]
pub enum ProcessAction {
    /// Convert the capture body into a task in the given project.
    ConvertToTask {
        /// Destination project slug.
        project_slug: String,
        /// Optional task title (defaults to the capture body if `None`).
        title: Option<String>,
        /// Optional task priority (defaults to `Medium` if `None`).
        priority: Option<TaskPriority>,
    },
    /// Convert the capture body into a todo in the given project.
    ConvertToTodo {
        /// Destination project slug.
        project_slug: String,
        /// Optional todo title (defaults to the capture body if `None`).
        title: Option<String>,
    },
    /// Assign the capture item to a project without converting.
    ///
    /// The item is marked processed; no task or todo is created.
    AssignToProject {
        /// Destination project slug.
        project_slug: String,
    },
    /// Discard the capture item without creating any entity.
    Discard,
}

/// High-level inbox operations for capture and processing.
///
/// Construct via [`InboxOps::new`], passing the shared database connection.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::ops::InboxOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let ops = InboxOps::new(conn);
/// ```
#[derive(Clone, Debug)]
pub struct InboxOps {
    captures: SqliteCaptureItems,
    projects: SqliteProjects,
    todos: SqliteTodos,
    task_ops: TaskOps,
}

impl InboxOps {
    /// Creates a new [`InboxOps`] backed by the given connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::ops::InboxOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let ops = InboxOps::new(&conn);
    /// ```
    #[must_use]
    pub fn new(conn: &Arc<Mutex<Connection>>) -> Self {
        Self {
            captures: SqliteCaptureItems::new(Arc::clone(conn)),
            projects: SqliteProjects::new(Arc::clone(conn)),
            todos: SqliteTodos::new(Arc::clone(conn)),
            task_ops: TaskOps::new(Arc::clone(conn)),
        }
    }

    /// Captures a thought and stores it in the inbox.
    ///
    /// Trims leading and trailing whitespace from `body`. Returns an error for
    /// empty bodies.
    ///
    /// # Errors
    ///
    /// Returns an error if `body` is empty after trimming, slug generation
    /// fails, or a database error occurs.
    pub fn capture(&self, body: &str) -> anyhow::Result<CaptureItem> {
        let body = body.trim();
        if body.is_empty() {
            return Err(anyhow::anyhow!("capture body must not be empty"));
        }

        let now = Utc::now();
        // Slug format: capture-{YYYYMMDD}-{HHmmss}
        let date_part = now.format("%Y%m%d").to_string();
        let time_part = now.format("%H%M%S").to_string();
        let base_slug = format!("capture-{date_part}-{time_part}");
        let unique_slug = slug::ensure_unique(&base_slug, |candidate| {
            self.captures
                .find_by_slug(candidate)
                .map(|r| r.is_some())
                .unwrap_or(false)
        })
        .map_err(|e| anyhow::anyhow!("slug generation failed: {e}"))?;

        self.captures.create(NewCaptureItem {
            slug: unique_slug,
            body: body.to_owned(),
            created_at: now,
        })
    }

    /// Lists capture items.
    ///
    /// When `include_processed` is `false`, only unprocessed items are
    /// returned.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn list(&self, include_processed: bool) -> anyhow::Result<Vec<CaptureItem>> {
        self.captures.list(include_processed)
    }

    /// Returns the capture item with the given `slug`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn get(&self, item_slug: &str) -> anyhow::Result<Option<CaptureItem>> {
        self.captures.find_by_slug(item_slug)
    }

    /// Processes a capture item by executing `action` and marking it processed.
    ///
    /// Depending on the action:
    /// - [`ProcessAction::ConvertToTask`] — creates a task and marks processed.
    /// - [`ProcessAction::ConvertToTodo`] — creates a todo and marks processed.
    /// - [`ProcessAction::AssignToProject`] — validates the project and marks processed.
    /// - [`ProcessAction::Discard`] — marks processed immediately (no entity created).
    ///
    /// # Errors
    ///
    /// Returns an error if the capture item does not exist, the target project
    /// does not exist or is archived, or a database error occurs.
    pub fn process(&self, item_slug: &str, action: ProcessAction) -> anyhow::Result<CaptureItem> {
        let item = self
            .captures
            .find_by_slug(item_slug)?
            .ok_or_else(|| anyhow::anyhow!("capture item '{item_slug}' not found"))?;

        match action {
            ProcessAction::ConvertToTask {
                project_slug,
                title,
                priority,
            } => {
                let project = self
                    .projects
                    .find_by_slug(&project_slug)?
                    .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;

                if project.archived_at.is_some() {
                    return Err(anyhow::anyhow!(
                        "project '{project_slug}' is archived; restore it first"
                    ));
                }

                let task_title = title.unwrap_or_else(|| item.body.clone());
                self.task_ops.create_task(CreateTask {
                    project_slug: project.slug.clone(),
                    project_id: project.id,
                    title: task_title,
                    description: None,
                    status: TaskStatus::Todo,
                    priority: priority.unwrap_or(TaskPriority::Medium),
                    due_date: None,
                })?;
            }
            ProcessAction::ConvertToTodo {
                project_slug,
                title,
            } => {
                let project = self
                    .projects
                    .find_by_slug(&project_slug)?
                    .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;

                if project.archived_at.is_some() {
                    return Err(anyhow::anyhow!(
                        "project '{project_slug}' is archived; restore it first"
                    ));
                }

                let todo_title = title.unwrap_or_else(|| item.body.clone());
                let prefix = format!("{}-todo-", project.slug);
                let base_slug = slug::generate(&prefix, &todo_title);
                let unique_slug = slug::ensure_unique(&base_slug, |candidate| {
                    self.todos
                        .find_by_slug(candidate)
                        .map(|r| r.is_some())
                        .unwrap_or(false)
                })
                .map_err(|e| anyhow::anyhow!("slug generation failed: {e}"))?;

                self.todos.create(NewTodo {
                    slug: unique_slug,
                    project_id: project.id,
                    title: todo_title,
                })?;
            }
            ProcessAction::AssignToProject { project_slug } => {
                let project = self
                    .projects
                    .find_by_slug(&project_slug)?
                    .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;

                if project.archived_at.is_some() {
                    return Err(anyhow::anyhow!(
                        "project '{project_slug}' is archived; restore it first"
                    ));
                }
                // The assignment is logical only (no FK on capture_items).
                // Future phases may persist project_id on capture items.
            }
            ProcessAction::Discard => {}
        }

        self.captures.mark_processed(item_slug)
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn ops() -> InboxOps {
        let conn = Arc::new(Mutex::new(open_in_memory().expect("in-memory db")));
        InboxOps::new(&conn)
    }

    #[test]
    fn test_capture_creates_item() {
        let ops = ops();
        let item = ops.capture("Buy groceries").expect("capture");
        assert_eq!(item.body, "Buy groceries");
        assert!(!item.processed);
        assert!(item.slug.starts_with("capture-"));
    }

    #[test]
    fn test_capture_trims_whitespace() {
        let ops = ops();
        let item = ops.capture("  hello  ").expect("capture");
        assert_eq!(item.body, "hello");
    }

    #[test]
    fn test_capture_empty_body_returns_error() {
        let ops = ops();
        let err = ops.capture("   ").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_process_discard() {
        let ops = ops();
        let item = ops.capture("Discard me").expect("capture");
        let processed = ops
            .process(&item.slug, ProcessAction::Discard)
            .expect("process");
        assert!(processed.processed);
    }

    #[test]
    fn test_process_convert_to_todo() {
        let ops = ops();
        let item = ops.capture("Convert to todo").expect("capture");
        let processed = ops
            .process(
                &item.slug,
                ProcessAction::ConvertToTodo {
                    project_slug: "quick-capture".to_owned(),
                    title: None,
                },
            )
            .expect("process");
        assert!(processed.processed);
    }

    #[test]
    fn test_process_project_not_found_returns_error() {
        let ops = ops();
        let item = ops.capture("No project").expect("capture");
        let err = ops.process(&item.slug, ProcessAction::Discard).map(|_| ());
        // Discard always succeeds
        assert!(
            err.is_ok(),
            "Discard should have succeeded, but got error: {:?}",
            err.err()
        );

        let item2 = ops.capture("No project 2").expect("capture");
        let err2 = ops
            .process(
                &item2.slug,
                ProcessAction::AssignToProject {
                    project_slug: "nonexistent".to_owned(),
                },
            )
            .unwrap_err();
        assert!(err2.to_string().contains("not found"));
    }
}
