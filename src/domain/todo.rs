// Rust guideline compliant 2026-02-21
//! `Todo` entity and the `Todos` repository trait.
//!
//! Todos are lightweight checklist items. They belong to a project and have a
//! simple `done` boolean rather than a multi-state status. Slugs are
//! auto-generated, e.g. `payments-todo-review-docs`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{ProjectId, TodoId};

// ── entity struct ──────────────────────────────────────────────────────────

/// A todo record as stored in the database.
// Phase 2+: not yet constructed in production code paths.
#[allow(dead_code, reason = "used in Phase 2 todo feature")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Todo {
    /// Internal numeric primary key (not exposed to users).
    pub id: TodoId,
    /// Unique slug, e.g. `payments-todo-review-docs`.
    pub slug: String,
    /// The project this todo belongs to.
    pub project_id: ProjectId,
    /// Short description of the item.
    pub title: String,
    /// Whether the item has been completed.
    pub done: bool,
    /// Timestamp when archived; `None` means the todo is active.
    pub archived_at: Option<DateTime<Utc>>,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Last-updated timestamp (UTC).
    pub updated_at: DateTime<Utc>,
}

// ── repository trait ───────────────────────────────────────────────────────

/// Data-access operations for the `todos` table.
// Phase 2+: not yet used in production paths.
#[allow(dead_code, reason = "used in Phase 2 todo feature")]
pub trait Todos {
    /// Inserts a new todo and returns the persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug already exists or a database error occurs.
    fn create(&self, todo: NewTodo) -> anyhow::Result<Todo>;

    /// Looks up a todo by its slug.
    ///
    /// Returns `Ok(None)` when no todo with that slug exists.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Todo>>;

    /// Lists todos, with optional filtering.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list(
        &self,
        project_id: Option<ProjectId>,
        include_done: bool,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Todo>>;

    /// Updates mutable fields of an existing todo.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    fn update(&self, slug: &str, patch: TodoPatch) -> anyhow::Result<Todo>;

    /// Archives the todo identified by `slug`.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    fn archive(&self, slug: &str) -> anyhow::Result<Todo>;

    /// Restores an archived todo.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    fn restore(&self, slug: &str) -> anyhow::Result<Todo>;

    /// Permanently deletes the todo row from the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    fn delete(&self, slug: &str) -> anyhow::Result<()>;

    /// Archives all todos belonging to the given project.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()>;
}

// ── input types ────────────────────────────────────────────────────────────

/// Parameters required to create a new todo.
// Phase 2+: not yet constructed in production code paths.
#[allow(dead_code, reason = "used in Phase 2 todo feature")]
#[derive(Debug, Clone)]
pub struct NewTodo {
    /// Pre-generated unique slug.
    pub slug: String,
    /// Owning project.
    pub project_id: ProjectId,
    /// Short description.
    pub title: String,
}

/// Partial update for mutable todo fields.
// Phase 2+: not yet constructed in production code paths.
#[allow(dead_code, reason = "used in Phase 2 todo feature")]
#[derive(Debug, Clone, Default)]
pub struct TodoPatch {
    /// New title, if changing.
    pub title: Option<String>,
    /// New done state, if changing.
    pub done: Option<bool>,
    /// New project assignment, if moving.
    pub project_id: Option<ProjectId>,
}
