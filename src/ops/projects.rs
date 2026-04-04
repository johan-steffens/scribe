//! Business logic operations for the project entity.
//!
//! [`ProjectOps`] wraps the `SqliteProjects` store and adds rules such as:
//! - Blocking delete/archive of reserved projects (enforced in the store, but
//!   also checked here for early, descriptive errors).
//! - Cascade-archiving all linked tasks, todos, time entries, and reminders
//!   when a project is archived.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::domain::{
    NewProject, Project, ProjectId, ProjectPatch, ProjectStatus, Projects, Reminders, Tasks,
    TimeEntries, Todos,
};
use crate::store::{SqliteProjects, SqliteReminders, SqliteTasks, SqliteTimeEntries, SqliteTodos};

/// High-level operations on projects, including cascade archive behaviour.
///
/// Construct via [`ProjectOps::new`], passing the shared database connection.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::ops::ProjectOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let ops = ProjectOps::new(&conn);
/// ```
#[derive(Clone, Debug)]
pub struct ProjectOps {
    projects: SqliteProjects,
    tasks: SqliteTasks,
    todos: SqliteTodos,
    time_entries: SqliteTimeEntries,
    reminders: SqliteReminders,
}

impl ProjectOps {
    /// Creates a new [`ProjectOps`] backed by the given connection.
    ///
    /// All store instances share the same `Arc<Mutex<Connection>>`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::ops::ProjectOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let ops = ProjectOps::new(&conn);
    /// ```
    #[must_use]
    pub fn new(conn: &Arc<Mutex<Connection>>) -> Self {
        Self {
            projects: SqliteProjects::new(Arc::clone(conn)),
            tasks: SqliteTasks::new(Arc::clone(conn)),
            todos: SqliteTodos::new(Arc::clone(conn)),
            time_entries: SqliteTimeEntries::new(Arc::clone(conn)),
            reminders: SqliteReminders::new(Arc::clone(conn)),
        }
    }

    /// Creates a new project with the given parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug is already in use or a database error
    /// occurs.
    pub fn create_project(&self, project: NewProject) -> anyhow::Result<Project> {
        self.projects.create(project)
    }

    /// Returns the project with the given `slug`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn get_project(&self, slug: &str) -> anyhow::Result<Option<Project>> {
        self.projects.find_by_slug(slug)
    }

    /// Lists projects filtered by status and archive state.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn list_projects(
        &self,
        status: Option<ProjectStatus>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Project>> {
        self.projects.list(status, include_archived)
    }

    /// Updates mutable fields of an existing project.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist or a database error
    /// occurs.
    pub fn update_project(&self, slug: &str, patch: ProjectPatch) -> anyhow::Result<Project> {
        self.projects.update(slug, patch)
    }

    /// Archives a project and all its linked items in sequence.
    ///
    /// Items archived: tasks, todos, time entries, reminders.
    /// Reserved projects cannot be archived.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is reserved, does not exist, or a
    /// database error occurs.
    pub fn archive_project(&self, slug: &str) -> anyhow::Result<Project> {
        // Resolve slug → id for cascaded archive calls.
        let project = self
            .projects
            .find_by_slug(slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{slug}' not found"))?;

        if project.is_reserved {
            return Err(anyhow::anyhow!(
                "project '{slug}' is reserved and cannot be archived"
            ));
        }

        let pid: ProjectId = project.id;

        // Cascade archive all linked items before archiving the project.
        self.tasks.archive_all_for_project(pid)?;
        self.todos.archive_all_for_project(pid)?;
        self.time_entries.archive_all_for_project(pid)?;
        self.reminders.archive_all_for_project(pid)?;

        self.projects.archive(slug)
    }

    /// Restores an archived project.
    ///
    /// Note: linked items are **not** automatically restored — they must be
    /// restored individually.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist or a database error
    /// occurs.
    pub fn restore_project(&self, slug: &str) -> anyhow::Result<Project> {
        self.projects.restore(slug)
    }

    /// Deletes a project.
    ///
    /// Blocked for reserved projects and projects that still have linked items.
    /// The caller must archive (cascade) before deletion if items exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is reserved, has linked items, does not
    /// exist, or a database error occurs.
    pub fn delete_project(&self, slug: &str) -> anyhow::Result<()> {
        self.projects.delete(slug)
    }
}

// ── test helpers ─────────────────────────────────────────────────────────

#[cfg(feature = "test-util")]
pub mod testing {
    //! Test helpers for the project ops module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::ProjectOps`] instances against an in-memory database.

    use super::{Arc, Mutex, NewProject, ProjectOps, ProjectStatus};
    use crate::db::open_in_memory;

    /// Constructs a [`ProjectOps`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn ops() -> ProjectOps {
        let conn = Arc::new(Mutex::new(open_in_memory().expect("in-memory db")));
        ProjectOps::new(&conn)
    }

    /// Creates a new [`NewProject`] with the given slug.
    #[must_use]
    pub fn new_project(slug: &str) -> NewProject {
        NewProject {
            slug: slug.to_owned(),
            name: slug.to_owned(),
            description: None,
            status: ProjectStatus::Active,
        }
    }
}
