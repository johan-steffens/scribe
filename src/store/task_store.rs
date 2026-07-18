//! `SQLite` implementation of the [`Tasks`] repository trait.
//!
//! [`SqliteTasks`] wraps a shared `Arc<Mutex<Connection>>` and provides
//! full CRUD plus archive/restore operations for the `tasks` table.

use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, Utc};
use rusqlite::types::ToSql;
use rusqlite::{Connection, params};

use crate::domain::{
    NewTask, ProjectId, Task, TaskId, TaskKind, TaskPatch, TaskPriority, TaskStatus, Tasks,
};
use crate::store::project_store::{parse_dt, parse_dt_opt};

// ── row mapping ────────────────────────────────────────────────────────────

const SELECT_COLS: &str = "id, slug, project_id, title, description, status, priority, \
     due_date, parent_id, kind, archived_at, created_at, updated_at";

struct RawRow {
    id: i64,
    slug: String,
    project_id: i64,
    title: String,
    description: Option<String>,
    status: String,
    priority: String,
    due_date: Option<String>,
    parent_id: Option<i64>,
    kind: String,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RawRow {
    fn into_task(
        self,
        project_slug: Option<String>,
        parent_slug: Option<String>,
    ) -> anyhow::Result<Task> {
        let project_slug = project_slug.unwrap_or_else(|| "unknown".to_owned());
        let due_date = self
            .due_date
            .map(|s| {
                NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                    .map_err(|e| anyhow::anyhow!("invalid due_date '{s}': {e}"))
            })
            .transpose()?;

        Ok(Task {
            id: TaskId(self.id),
            slug: self.slug,
            project_id: ProjectId(self.project_id),
            project_slug,
            title: self.title,
            description: self.description,
            status: self
                .status
                .parse::<TaskStatus>()
                .map_err(|e| anyhow::anyhow!(e))?,
            priority: self
                .priority
                .parse::<TaskPriority>()
                .map_err(|e| anyhow::anyhow!(e))?,
            due_date,
            parent_id: self.parent_id.map(TaskId),
            parent_slug,
            kind: self
                .kind
                .parse::<TaskKind>()
                .map_err(|e| anyhow::anyhow!(e))?,
            archived_at: parse_dt_opt(self.archived_at)?,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
        })
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        slug: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        due_date: row.get(7)?,
        parent_id: row.get(8)?,
        kind: row.get(9)?,
        archived_at: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

#[cfg(feature = "sync")]
fn map_row_with_slugs(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(RawRow, String, Option<String>)> {
    Ok((
        RawRow {
            id: row.get(0)?,
            slug: row.get(1)?,
            project_id: row.get(2)?,
            title: row.get(3)?,
            description: row.get(4)?,
            status: row.get(5)?,
            priority: row.get(6)?,
            due_date: row.get(7)?,
            parent_id: row.get(8)?,
            kind: row.get(9)?,
            archived_at: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        },
        row.get(13)?,
        row.get(14)?,
    ))
}

fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<Task>> {
    let sql = format!("SELECT {SELECT_COLS} FROM tasks WHERE slug = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut iter = stmt.query_map(params![slug], map_row)?;
    iter.next()
        .transpose()
        .map_err(anyhow::Error::from)?
        .map(|raw| {
            let project_slug = {
                let mut s = conn.prepare(
                    "SELECT p.slug FROM projects p JOIN tasks t ON t.project_id = p.id WHERE t.slug = ?1",
                )?;
                s.query_row(params![slug], |row| row.get(0)).ok()
            };
            let parent_slug = {
                let mut s = conn.prepare(
                    "SELECT parent.slug FROM tasks child \
                     JOIN tasks parent ON child.parent_id = parent.id \
                     WHERE child.slug = ?1",
                )?;
                s.query_row(params![slug], |row| row.get(0)).ok()
            };
            raw.into_task(project_slug, parent_slug)
        })
        .transpose()
}

// ── SqliteTasks ────────────────────────────────────────────────────────────

/// `SQLite`-backed implementation of the [`Tasks`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
#[derive(Clone, Debug)]
pub struct SqliteTasks {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteTasks {
    /// Creates a new [`SqliteTasks`] wrapping the given shared connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteTasks;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteTasks::new(conn);
    /// ```
    #[must_use]
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn lock(&self) -> anyhow::Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))
    }
}

impl Tasks for SqliteTasks {
    fn create(&self, task: NewTask) -> anyhow::Result<Task> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let due_date_str = task.due_date.map(|d| d.format("%Y-%m-%d").to_string());
        conn.execute(
            "INSERT INTO tasks \
             (slug, project_id, title, description, status, priority, \
              due_date, parent_id, kind, created_at, updated_at) \
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.slug,
                task.project_id.0,
                task.title,
                task.description,
                task.status.to_string(),
                task.priority.to_string(),
                due_date_str,
                task.parent_id.map(|id| id.0),
                task.kind.to_string(),
                now.clone(),
                now,
            ],
        )?;
        fetch_one(&conn, &task.slug)?
            .ok_or_else(|| anyhow::anyhow!("task '{}' not found after insert", task.slug))
    }

    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Task>> {
        let conn = self.lock()?;
        fetch_one(&conn, slug)
    }

    fn list(
        &self,
        project_id: Option<ProjectId>,
        status: Option<TaskStatus>,
        priority: Option<TaskPriority>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Task>> {
        let conn = self.lock()?;
        let mut conditions: Vec<String> = Vec::new();
        if !include_archived {
            conditions.push("archived_at IS NULL".to_owned());
        }
        if let Some(pid) = project_id {
            conditions.push(format!("project_id = {}", pid.0));
        }
        if let Some(s) = &status {
            conditions.push(format!("status = '{s}'"));
        }
        if let Some(p) = &priority {
            conditions.push(format!("priority = '{p}'"));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!("SELECT {SELECT_COLS} FROM tasks {where_clause} ORDER BY created_at");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_task(None, None))
            .collect()
    }

    fn update(&self, slug: &str, patch: TaskPatch) -> anyhow::Result<Task> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();

        let mut sets: Vec<String> = vec!["updated_at = ?1".to_owned()];
        let mut extra: Vec<Option<String>> = Vec::new();

        if let Some(ref v) = patch.title {
            let i = extra.len() + 2;
            sets.push(format!("title = ?{i}"));
            extra.push(Some(v.clone()));
        }
        if let Some(ref v) = patch.description {
            let i = extra.len() + 2;
            sets.push(format!("description = ?{i}"));
            extra.push(Some(v.clone()));
        } else if patch.clear_description {
            let i = extra.len() + 2;
            sets.push(format!("description = ?{i}"));
            extra.push(None);
        }
        if let Some(ref v) = patch.status {
            let i = extra.len() + 2;
            sets.push(format!("status = ?{i}"));
            extra.push(Some(v.to_string()));
        }
        if let Some(ref v) = patch.priority {
            let i = extra.len() + 2;
            sets.push(format!("priority = ?{i}"));
            extra.push(Some(v.to_string()));
        }
        if let Some(ref v) = patch.due_date {
            let i = extra.len() + 2;
            sets.push(format!("due_date = ?{i}"));
            extra.push(Some(v.format("%Y-%m-%d").to_string()));
        } else if patch.clear_due_date {
            let i = extra.len() + 2;
            sets.push(format!("due_date = ?{i}"));
            extra.push(None);
        }
        if let Some(ref v) = patch.project_id {
            let i = extra.len() + 2;
            sets.push(format!("project_id = ?{i}"));
            extra.push(Some(v.0.to_string()));
        }
        if let Some(ref v) = patch.parent_id {
            let i = extra.len() + 2;
            sets.push(format!("parent_id = ?{i}"));
            extra.push(Some(v.0.to_string()));
        } else if patch.clear_parent_id {
            let i = extra.len() + 2;
            sets.push(format!("parent_id = ?{i}"));
            extra.push(None);
        }
        if let Some(ref v) = patch.kind {
            let i = extra.len() + 2;
            sets.push(format!("kind = ?{i}"));
            extra.push(Some(v.to_string()));
        }

        let where_i = extra.len() + 2;
        let sql = format!(
            "UPDATE tasks SET {} WHERE slug = ?{where_i}",
            sets.join(", ")
        );

        let mut all_params: Vec<Option<String>> = vec![Some(now)];
        all_params.extend(extra);
        all_params.push(Some(slug.to_owned()));

        let sql_params: Vec<&dyn ToSql> = all_params.iter().map(|v| v as &dyn ToSql).collect();
        let rows = conn.execute(&sql, sql_params.as_slice())?;
        if rows == 0 {
            return Err(anyhow::anyhow!("task '{slug}' not found"));
        }
        fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("task '{slug}' not found after update"))
    }

    fn archive(&self, slug: &str) -> anyhow::Result<Task> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE tasks SET archived_at = ?1, updated_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("task '{slug}' not found"));
        }
        fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("task '{slug}' not found after archive"))
    }

    fn restore(&self, slug: &str) -> anyhow::Result<Task> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE tasks SET archived_at = NULL, updated_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("task '{slug}' not found"));
        }
        fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("task '{slug}' not found after restore"))
    }

    fn delete(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let rows = conn.execute("DELETE FROM tasks WHERE slug = ?1", params![slug])?;
        if rows == 0 {
            return Err(anyhow::anyhow!("task '{slug}' not found"));
        }
        Ok(())
    }

    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE tasks SET archived_at = ?1, updated_at = ?1 \
             WHERE project_id = ?2 AND archived_at IS NULL",
            params![now, project_id.0],
        )?;
        Ok(())
    }
}

// ── test helpers ─────────────────────────────────────────────────────────

pub mod testing {
    //! Test helpers for the task store module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::SqliteTasks`] instances against an in-memory database.

    use super::{Arc, Mutex, NewTask, ProjectId, SqliteTasks, TaskKind, TaskPriority, TaskStatus};
    use crate::db::open_in_memory;

    /// The seeded quick-capture project ID used in tests.
    pub const QC_PROJECT_ID: ProjectId = ProjectId(1);

    /// Constructs a [`SqliteTasks`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn store() -> SqliteTasks {
        let conn = open_in_memory().expect("in-memory db");
        SqliteTasks::new(Arc::new(Mutex::new(conn)))
    }

    /// Creates a [`NewTask`] for testing purposes.
    #[must_use]
    pub fn new_task(slug: &str, title: &str) -> NewTask {
        NewTask {
            slug: slug.to_owned(),
            project_id: QC_PROJECT_ID,
            title: title.to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
            kind: TaskKind::Task,
        }
    }
}

#[cfg(feature = "sync")]
impl SqliteTasks {
    /// Returns every task row, including archived ones.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_all(&self) -> anyhow::Result<Vec<Task>> {
        let conn = self.lock()?;
        // LEFT JOIN parent so hierarchical tasks carry a portable parent_slug
        // for multi-device sync (local parent_id values are not portable).
        let sql = "SELECT t.id, t.slug, t.project_id, t.title, t.description, t.status, \
             t.priority, t.due_date, t.parent_id, t.kind, t.archived_at, t.created_at, \
             t.updated_at, p.slug, parent.slug \
             FROM tasks t \
             JOIN projects p ON t.project_id = p.id \
             LEFT JOIN tasks parent ON t.parent_id = parent.id \
             ORDER BY t.created_at ASC";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], map_row_with_slugs)?;
        rows.map(|r| {
            let (raw, project_slug, parent_slug) = r.map_err(anyhow::Error::from)?;
            raw.into_task(Some(project_slug), parent_slug)
        })
        .collect()
    }

    /// Resolves a project slug to its local numeric ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the project with the given slug does not exist.
    fn resolve_project_id(conn: &Connection, project_slug: &str) -> anyhow::Result<ProjectId> {
        let id = conn
            .query_row(
                "SELECT id FROM projects WHERE slug = ?1",
                params![project_slug],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| anyhow::anyhow!("project '{project_slug}' not found: {e}"))?;
        Ok(ProjectId(id))
    }

    /// Resolves a task slug to its local numeric ID, if present.
    fn resolve_task_id(conn: &Connection, task_slug: &str) -> anyhow::Result<Option<TaskId>> {
        let result = conn.query_row(
            "SELECT id FROM tasks WHERE slug = ?1",
            params![task_slug],
            |row| row.get::<_, i64>(0),
        );
        match result {
            Ok(id) => Ok(Some(TaskId(id))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Inserts or updates each task by slug, resolving project and parent slugs
    /// to local IDs.
    ///
    /// Hierarchy is applied in a second pass so parents do not need to appear
    /// before children in the input slice. `parent_id` from the remote is
    /// **ignored** — only `parent_slug` is portable across databases.
    ///
    /// # Errors
    ///
    /// Returns an error if any project slug cannot be resolved or any database
    /// write fails.
    pub fn upsert_all_with_slug_resolution(&self, tasks: &[Task]) -> anyhow::Result<()> {
        let mut conn = self.lock()?;

        let task_data: Vec<_> = tasks
            .iter()
            .map(|t| {
                let local_project_id = Self::resolve_project_id(&conn, &t.project_slug)?;
                let due_date_str = t.due_date.map(|d| d.format("%Y-%m-%d").to_string());
                Ok((local_project_id, due_date_str, t))
            })
            .collect::<anyhow::Result<_>>()?;

        let tx = conn.transaction()?;
        // Pass 1: upsert rows without parent_id so children can be written
        // before parents exist locally.
        for (local_project_id, due_date_str, t) in &task_data {
            tx.execute(
                "INSERT INTO tasks \
                 (slug, project_id, title, description, status, priority, \
                  due_date, parent_id, kind, archived_at, created_at, updated_at) \
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11) \
                  ON CONFLICT(slug) DO UPDATE SET \
                    project_id  = excluded.project_id, \
                    title       = excluded.title, \
                    description = excluded.description, \
                    status      = excluded.status, \
                    priority    = excluded.priority, \
                    due_date    = excluded.due_date, \
                    kind        = excluded.kind, \
                    archived_at = excluded.archived_at, \
                    updated_at  = excluded.updated_at",
                rusqlite::params![
                    t.slug,
                    local_project_id.0,
                    t.title,
                    t.description,
                    t.status.to_string(),
                    t.priority.to_string(),
                    due_date_str,
                    t.kind.to_string(),
                    t.archived_at.map(|dt| dt.to_rfc3339()),
                    t.created_at.to_rfc3339(),
                    t.updated_at.to_rfc3339(),
                ],
            )?;
        }
        // Pass 2: resolve parent_slug → local parent_id (or clear parent).
        for (_, _, t) in &task_data {
            if let Some(parent_slug) = t.parent_slug.as_deref() {
                let local_parent_id = Self::resolve_task_id(&tx, parent_slug)?
                    .ok_or_else(|| anyhow::anyhow!("parent task '{parent_slug}' not found"))?
                    .0;
                tx.execute(
                    "UPDATE tasks SET parent_id = ?1 WHERE slug = ?2",
                    params![local_parent_id, t.slug],
                )?;
            } else {
                // Explicit clear — top-level task.
                tx.execute(
                    "UPDATE tasks SET parent_id = NULL WHERE slug = ?1",
                    params![t.slug],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
