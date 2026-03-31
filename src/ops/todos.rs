// Rust guideline compliant 2026-02-21
//! Business logic operations for the todo entity.
//!
//! [`TodoOps`] wraps `SqliteTodos` and adds rules such as:
//! - Validating that the owning project exists and is not archived before
//!   creating a todo.
//! - Validating destination projects when moving a todo.
//! - Blocking hard delete on non-archived todos.
//!
//! Slug generation follows the pattern `{project_slug}-todo-{title_slug}`.
//!
//! # TODO(phase3): migrate to domain error structs per M-ERRORS-CANONICAL-STRUCTS

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::domain::Projects;
use crate::domain::{NewTodo, ProjectId, Todo, TodoPatch, Todos, slug};
use crate::store::{SqliteProjects, SqliteTodos};

/// High-level todo operations with project validation and slug generation.
///
/// Construct via [`TodoOps::new`], passing the shared database connection.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::ops::TodoOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let ops = TodoOps::new(conn);
/// ```
#[derive(Clone, Debug)]
pub struct TodoOps {
    todos: SqliteTodos,
    projects: SqliteProjects,
}

impl TodoOps {
    /// Creates a new [`TodoOps`] backed by the given connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::ops::TodoOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let ops = TodoOps::new(conn);
    /// ```
    #[must_use]
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            todos: SqliteTodos::new(Arc::clone(&conn)),
            projects: SqliteProjects::new(conn),
        }
    }

    /// Creates a new todo under the given project slug.
    ///
    /// The slug is auto-generated as `{project_slug}-todo-{title_slug}`.
    /// Returns an error if the project does not exist or is archived.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is not found, is archived, slug
    /// generation fails after retries, or a database error occurs.
    pub fn create(&self, project_slug: &str, title: &str) -> anyhow::Result<Todo> {
        let project = self
            .projects
            .find_by_slug(project_slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;

        if project.archived_at.is_some() {
            return Err(anyhow::anyhow!(
                "project '{project_slug}' is archived; restore it before adding todos"
            ));
        }

        let prefix = format!("{project_slug}-todo-");
        let base_slug = slug::generate(&prefix, title);
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
            title: title.to_owned(),
        })
    }

    /// Returns the todo with the given `slug`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn get(&self, slug: &str) -> anyhow::Result<Option<Todo>> {
        self.todos.find_by_slug(slug)
    }

    /// Lists todos with optional project filter.
    ///
    /// When `include_done` is `false`, completed todos are excluded.
    /// When `include_archived` is `true`, archived todos are included.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn list(
        &self,
        project_id: Option<ProjectId>,
        include_done: bool,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Todo>> {
        self.todos.list(project_id, include_done, include_archived)
    }

    /// Marks a todo as done.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    pub fn mark_done(&self, todo_slug: &str) -> anyhow::Result<Todo> {
        self.todos.update(
            todo_slug,
            TodoPatch {
                done: Some(true),
                ..Default::default()
            },
        )
    }

    /// Marks a previously-done todo as undone.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    pub fn mark_undone(&self, todo_slug: &str) -> anyhow::Result<Todo> {
        self.todos.update(
            todo_slug,
            TodoPatch {
                done: Some(false),
                ..Default::default()
            },
        )
    }

    /// Updates the title of an existing todo.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    pub fn update_title(&self, todo_slug: &str, new_title: &str) -> anyhow::Result<Todo> {
        self.todos.update(
            todo_slug,
            TodoPatch {
                title: Some(new_title.to_owned()),
                ..Default::default()
            },
        )
    }

    /// Moves a todo to a different project.
    ///
    /// The destination project must exist and not be archived.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo or destination project does not exist, if
    /// the destination is archived, or a database error occurs.
    pub fn move_project(&self, todo_slug: &str, dest_project_slug: &str) -> anyhow::Result<Todo> {
        let project = self
            .projects
            .find_by_slug(dest_project_slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{dest_project_slug}' not found"))?;

        if project.archived_at.is_some() {
            return Err(anyhow::anyhow!(
                "project '{dest_project_slug}' is archived; restore it first"
            ));
        }

        self.todos.update(
            todo_slug,
            TodoPatch {
                project_id: Some(project.id),
                ..Default::default()
            },
        )
    }

    /// Archives a todo.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    pub fn archive(&self, todo_slug: &str) -> anyhow::Result<Todo> {
        self.todos.archive(todo_slug)
    }

    /// Restores an archived todo.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo does not exist or a database error occurs.
    pub fn restore(&self, todo_slug: &str) -> anyhow::Result<Todo> {
        self.todos.restore(todo_slug)
    }

    /// Permanently deletes a todo.
    ///
    /// Only archived todos may be deleted. Pass `--force` to bypass this guard
    /// only after explicit user confirmation.
    ///
    /// # Errors
    ///
    /// Returns an error if the todo is not archived, does not exist, or a
    /// database error occurs.
    pub fn delete(&self, todo_slug: &str) -> anyhow::Result<()> {
        let todo = self
            .todos
            .find_by_slug(todo_slug)?
            .ok_or_else(|| anyhow::anyhow!("todo '{todo_slug}' not found"))?;

        if todo.archived_at.is_none() {
            return Err(anyhow::anyhow!(
                "todo '{todo_slug}' must be archived before deletion"
            ));
        }

        self.todos.delete(todo_slug)
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn ops() -> TodoOps {
        let conn = Arc::new(Mutex::new(open_in_memory().expect("in-memory db")));
        TodoOps::new(conn)
    }

    #[test]
    fn test_create_generates_slug() {
        let ops = ops();
        let todo = ops
            .create("quick-capture", "Buy groceries")
            .expect("create");
        assert_eq!(todo.slug, "quick-capture-todo-buy-groceries");
    }

    #[test]
    fn test_create_project_not_found_returns_error() {
        let ops = ops();
        let err = ops.create("nonexistent", "Title").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_mark_done() {
        let ops = ops();
        let todo = ops.create("quick-capture", "Do thing").expect("create");
        let done = ops.mark_done(&todo.slug).expect("done");
        assert!(done.done);
    }

    #[test]
    fn test_list_excludes_done_by_default() {
        let ops = ops();
        let todo = ops.create("quick-capture", "List test").expect("create");
        ops.mark_done(&todo.slug).expect("done");
        let active = ops.list(None, false, false).expect("list");
        assert!(!active.iter().any(|t| t.slug == todo.slug));
    }

    #[test]
    fn test_list_includes_done_when_requested() {
        let ops = ops();
        let todo = ops.create("quick-capture", "Include done").expect("create");
        ops.mark_done(&todo.slug).expect("done");
        let all = ops.list(None, true, false).expect("list");
        assert!(all.iter().any(|t| t.slug == todo.slug));
    }

    #[test]
    fn test_archive_and_restore() {
        let ops = ops();
        let todo = ops.create("quick-capture", "Archive me").expect("create");
        ops.archive(&todo.slug).expect("archive");
        let active = ops.list(None, true, false).expect("list");
        assert!(!active.iter().any(|t| t.slug == todo.slug));
        ops.restore(&todo.slug).expect("restore");
        let active = ops.list(None, true, false).expect("list");
        assert!(active.iter().any(|t| t.slug == todo.slug));
    }

    #[test]
    fn test_delete_requires_archived() {
        let ops = ops();
        let todo = ops.create("quick-capture", "Delete me").expect("create");
        let err = ops.delete(&todo.slug).unwrap_err();
        assert!(err.to_string().contains("archived"));
    }

    #[test]
    fn test_delete_archived_succeeds() {
        let ops = ops();
        let todo = ops
            .create("quick-capture", "Delete archived")
            .expect("create");
        ops.archive(&todo.slug).expect("archive");
        ops.delete(&todo.slug).expect("delete");
        assert!(ops.get(&todo.slug).expect("get").is_none());
    }

    #[test]
    fn test_mark_undone() {
        let ops = ops();
        let todo = ops.create("quick-capture", "Mark undone").expect("create");
        ops.mark_done(&todo.slug).expect("mark done");
        let undone = ops.mark_undone(&todo.slug).expect("mark undone");
        assert!(!undone.done);
    }

    #[test]
    fn test_update_title() {
        let ops = ops();
        let todo = ops.create("quick-capture", "Old title").expect("create");
        let updated = ops
            .update_title(&todo.slug, "New title")
            .expect("update title");
        assert_eq!(updated.title, "New title");
    }
}
