// Rust guideline compliant 2026-02-21
//! `SQLite` implementation of the [`Tasks`] repository trait.
//!
//! [`SqliteTasks`] wraps a shared `Arc<Mutex<Connection>>` and provides
//! full CRUD plus archive/restore operations for the `tasks` table.

use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, Utc};
use rusqlite::types::ToSql;
use rusqlite::{Connection, params};

use crate::domain::{NewTask, ProjectId, Task, TaskId, TaskPatch, TaskPriority, TaskStatus, Tasks};
use crate::store::project_store::{parse_dt, parse_dt_opt};

// ── row mapping ────────────────────────────────────────────────────────────

const SELECT_COLS: &str = "id, slug, project_id, title, description, status, priority, \
     due_date, archived_at, created_at, updated_at";

struct RawRow {
    id: i64,
    slug: String,
    project_id: i64,
    title: String,
    description: Option<String>,
    status: String,
    priority: String,
    due_date: Option<String>,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RawRow {
    fn into_task(self) -> anyhow::Result<Task> {
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
        archived_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<Task>> {
    let sql = format!("SELECT {SELECT_COLS} FROM tasks WHERE slug = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut iter = stmt.query_map(params![slug], map_row)?;
    iter.next()
        .transpose()
        .map_err(anyhow::Error::from)?
        .map(RawRow::into_task)
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
              due_date, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                task.slug,
                task.project_id.0,
                task.title,
                task.description,
                task.status.to_string(),
                task.priority.to_string(),
                due_date_str,
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
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_task())
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

#[cfg(feature = "sync")]
impl SqliteTasks {
    /// Returns every task row, including archived ones.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_all(&self) -> anyhow::Result<Vec<Task>> {
        let conn = self.lock()?;
        let sql = format!("SELECT {SELECT_COLS} FROM tasks ORDER BY created_at ASC");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_task())
            .collect()
    }

    /// Inserts or updates each task by slug.
    ///
    /// `slug` and `created_at` are write-once fields excluded from the update
    /// set. All other mutable fields are updated on conflict.
    ///
    /// # Errors
    ///
    /// Returns an error if any database write fails.
    pub fn upsert_all(&self, tasks: &[Task]) -> anyhow::Result<()> {
        let conn = self.lock()?;
        for t in tasks {
            let due_date_str = t.due_date.map(|d| d.format("%Y-%m-%d").to_string());
            conn.execute(
                "INSERT INTO tasks \
                 (slug, project_id, title, description, status, priority, \
                  due_date, archived_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT(slug) DO UPDATE SET \
                   project_id  = excluded.project_id, \
                   title       = excluded.title, \
                   description = excluded.description, \
                   status      = excluded.status, \
                   priority    = excluded.priority, \
                   due_date    = excluded.due_date, \
                   archived_at = excluded.archived_at, \
                   updated_at  = excluded.updated_at",
                rusqlite::params![
                    t.slug,
                    t.project_id.0,
                    t.title,
                    t.description,
                    t.status.to_string(),
                    t.priority.to_string(),
                    due_date_str,
                    t.archived_at.map(|dt| dt.to_rfc3339()),
                    t.created_at.to_rfc3339(),
                    t.updated_at.to_rfc3339(),
                ],
            )?;
        }
        Ok(())
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn store() -> SqliteTasks {
        let conn = open_in_memory().expect("in-memory db");
        SqliteTasks::new(Arc::new(Mutex::new(conn)))
    }

    // The seeded quick-capture project has id=1.
    fn qc_project() -> ProjectId {
        ProjectId(1)
    }

    fn new_task(slug: &str, title: &str) -> NewTask {
        NewTask {
            slug: slug.to_owned(),
            project_id: qc_project(),
            title: title.to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
        }
    }

    #[test]
    fn test_create_and_find() {
        let s = store();
        let t = s.create(new_task("qc-task-fix", "Fix it")).expect("create");
        assert_eq!(t.slug, "qc-task-fix");
        let found = s.find_by_slug("qc-task-fix").expect("find").expect("some");
        assert_eq!(found.id, t.id);
    }

    #[test]
    fn test_archive_and_restore() {
        let s = store();
        s.create(new_task("t1", "T1")).expect("create");
        s.archive("t1").expect("archive");
        let tasks = s.list(None, None, None, false).expect("list");
        assert!(!tasks.iter().any(|t| t.slug == "t1"));
        s.restore("t1").expect("restore");
        let tasks = s.list(None, None, None, false).expect("list");
        assert!(tasks.iter().any(|t| t.slug == "t1"));
    }

    #[test]
    fn test_delete() {
        let s = store();
        s.create(new_task("del", "Delete me")).expect("create");
        s.delete("del").expect("delete");
        assert!(s.find_by_slug("del").expect("find").is_none());
    }

    #[test]
    fn test_update_status() {
        let s = store();
        s.create(new_task("upd", "Update me")).expect("create");
        let t = s
            .update(
                "upd",
                TaskPatch {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                },
            )
            .expect("update");
        assert_eq!(t.status, TaskStatus::Done);
    }

    #[test]
    fn test_archive_all_for_project() {
        let s = store();
        s.create(new_task("p-t1", "T1")).expect("t1");
        s.create(new_task("p-t2", "T2")).expect("t2");
        s.archive_all_for_project(qc_project())
            .expect("archive all");
        let active = s.list(Some(qc_project()), None, None, false).expect("list");
        assert!(active.is_empty());
    }
}
