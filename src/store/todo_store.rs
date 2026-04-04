// Rust guideline compliant 2026-02-21
//! `SQLite` implementation of the [`Todos`] repository trait.
//!
//! Wired into the CLI via [`crate::ops::TodoOps`].

use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::types::ToSql;
use rusqlite::{Connection, params};

use crate::domain::{NewTodo, ProjectId, Todo, TodoId, TodoPatch, Todos};
use crate::store::project_store::{parse_dt, parse_dt_opt};

const SELECT_COLS: &str = "id, slug, project_id, title, done, archived_at, created_at, updated_at";

struct RawRow {
    id: i64,
    slug: String,
    project_id: i64,
    title: String,
    done: bool,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        slug: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        done: row.get::<_, i64>(4)? != 0,
        archived_at: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn map_row_with_project_slug(row: &rusqlite::Row<'_>) -> rusqlite::Result<(RawRow, String)> {
    Ok((
        RawRow {
            id: row.get(0)?,
            slug: row.get(1)?,
            project_id: row.get(2)?,
            title: row.get(3)?,
            done: row.get::<_, i64>(4)? != 0,
            archived_at: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        },
        row.get(8)?,
    ))
}

impl RawRow {
    fn into_todo(self, project_slug: Option<String>) -> anyhow::Result<Todo> {
        let project_slug = project_slug.unwrap_or_else(|| "unknown".to_owned());
        Ok(Todo {
            id: TodoId(self.id),
            slug: self.slug,
            project_id: ProjectId(self.project_id),
            project_slug,
            title: self.title,
            done: self.done,
            archived_at: parse_dt_opt(self.archived_at)?,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
        })
    }
}

/// `SQLite`-backed implementation of the [`Todos`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
#[derive(Clone, Debug)]
pub struct SqliteTodos {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteTodos {
    /// Creates a new [`SqliteTodos`] wrapping the given shared connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteTodos;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteTodos::new(conn);
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

    fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<Todo>> {
        let sql = format!("SELECT {SELECT_COLS} FROM todos WHERE slug = ?1");
        let mut stmt = conn.prepare(&sql)?;
        let mut iter = stmt.query_map(params![slug], map_row)?;
        iter.next()
            .transpose()
            .map_err(anyhow::Error::from)?
            .map(|raw| {
                let project_slug = {
                    let mut s = conn.prepare("SELECT p.slug FROM projects p JOIN todos t ON t.project_id = p.id WHERE t.slug = ?1")?;
                    s.query_row(params![slug], |row| row.get(0)).ok()
                };
                raw.into_todo(project_slug)
            })
            .transpose()
    }
}

impl Todos for SqliteTodos {
    fn create(&self, todo: NewTodo) -> anyhow::Result<Todo> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO todos (slug, project_id, title, done, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 0, ?4, ?4)",
            params![todo.slug, todo.project_id.0, todo.title, now],
        )?;
        Self::fetch_one(&conn, &todo.slug)?
            .ok_or_else(|| anyhow::anyhow!("todo '{}' not found after insert", todo.slug))
    }

    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Todo>> {
        let conn = self.lock()?;
        Self::fetch_one(&conn, slug)
    }

    fn list(
        &self,
        project_id: Option<ProjectId>,
        include_done: bool,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Todo>> {
        let conn = self.lock()?;
        let mut conditions: Vec<String> = Vec::new();
        if !include_archived {
            conditions.push("archived_at IS NULL".to_owned());
        }
        if !include_done {
            conditions.push("done = 0".to_owned());
        }
        if let Some(pid) = project_id {
            conditions.push(format!("project_id = {}", pid.0));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!("SELECT {SELECT_COLS} FROM todos {where_clause} ORDER BY created_at");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_todo(None))
            .collect()
    }

    fn update(&self, slug: &str, patch: TodoPatch) -> anyhow::Result<Todo> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let mut sets: Vec<String> = vec!["updated_at = ?1".to_owned()];
        let mut extra: Vec<Option<String>> = Vec::new();

        if let Some(ref v) = patch.title {
            let i = extra.len() + 2;
            sets.push(format!("title = ?{i}"));
            extra.push(Some(v.clone()));
        }
        if let Some(v) = patch.done {
            let i = extra.len() + 2;
            sets.push(format!("done = ?{i}"));
            extra.push(Some(if v { "1".to_owned() } else { "0".to_owned() }));
        }
        if let Some(ref v) = patch.project_id {
            let i = extra.len() + 2;
            sets.push(format!("project_id = ?{i}"));
            extra.push(Some(v.0.to_string()));
        }

        let where_i = extra.len() + 2;
        let sql = format!(
            "UPDATE todos SET {} WHERE slug = ?{where_i}",
            sets.join(", ")
        );

        let mut all_params: Vec<Option<String>> = vec![Some(now)];
        all_params.extend(extra);
        all_params.push(Some(slug.to_owned()));

        let sql_params: Vec<&dyn ToSql> = all_params.iter().map(|v| v as &dyn ToSql).collect();
        let rows = conn.execute(&sql, sql_params.as_slice())?;
        if rows == 0 {
            return Err(anyhow::anyhow!("todo '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("todo '{slug}' not found after update"))
    }

    fn archive(&self, slug: &str) -> anyhow::Result<Todo> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE todos SET archived_at = ?1, updated_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("todo '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("todo '{slug}' not found after archive"))
    }

    fn restore(&self, slug: &str) -> anyhow::Result<Todo> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE todos SET archived_at = NULL, updated_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("todo '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("todo '{slug}' not found after restore"))
    }

    fn delete(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let rows = conn.execute("DELETE FROM todos WHERE slug = ?1", params![slug])?;
        if rows == 0 {
            return Err(anyhow::anyhow!("todo '{slug}' not found"));
        }
        Ok(())
    }

    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE todos SET archived_at = ?1, updated_at = ?1 \
             WHERE project_id = ?2 AND archived_at IS NULL",
            params![now, project_id.0],
        )?;
        Ok(())
    }
}

// ── test helpers ─────────────────────────────────────────────────────────

#[cfg(test)]
pub mod testing {
    //! Test helpers for the todo store module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::SqliteTodos`] instances against an in-memory database.

    use super::*;
    use crate::db::open_in_memory;

    /// Constructs a [`SqliteTodos`] backed by an in-memory database.
    #[must_use]
    pub fn store() -> SqliteTodos {
        let conn = open_in_memory().expect("in-memory db");
        SqliteTodos::new(Arc::new(Mutex::new(conn)))
    }

    /// Creates a [`NewTodo`] for testing purposes.
    #[must_use]
    pub fn new_todo(slug: &str, title: &str) -> NewTodo {
        NewTodo {
            slug: slug.to_owned(),
            project_id: ProjectId(1),
            title: title.to_owned(),
        }
    }
}

#[cfg(feature = "sync")]
impl SqliteTodos {
    /// Returns every todo row, including archived and done ones.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_all(&self) -> anyhow::Result<Vec<Todo>> {
        let conn = self.lock()?;
        let sql = "SELECT t.id, t.slug, t.project_id, t.title, t.done, t.archived_at, \
             t.created_at, t.updated_at, p.slug \
             FROM todos t JOIN projects p ON t.project_id = p.id ORDER BY t.created_at ASC";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], map_row_with_project_slug)?;
        rows.map(|r| {
            let (raw, project_slug) = r.map_err(anyhow::Error::from)?;
            raw.into_todo(Some(project_slug))
        })
        .collect()
    }

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

    /// Inserts or updates each todo by slug, resolving project slug to local ID.
    ///
    /// This is the sync-safe version. It resolves `project_slug` to the local
    /// numeric ID before inserting, avoiding foreign key mismatches.
    ///
    /// # Errors
    ///
    /// Returns an error if any project slug cannot be resolved or if any
    /// database write fails.
    pub fn upsert_all_with_slug_resolution(&self, todos: &[Todo]) -> anyhow::Result<()> {
        let mut conn = self.lock()?;

        let todo_data: Vec<_> = todos
            .iter()
            .map(|t| {
                let local_project_id = Self::resolve_project_id(&conn, &t.project_slug)?;
                Ok((local_project_id, t))
            })
            .collect::<anyhow::Result<_>>()?;

        let tx = conn.transaction()?;
        for (local_project_id, t) in todo_data {
            tx.execute(
                "INSERT INTO todos \
                 (slug, project_id, title, done, archived_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                 ON CONFLICT(slug) DO UPDATE SET \
                   title       = excluded.title, \
                   done        = excluded.done, \
                   archived_at = excluded.archived_at, \
                   updated_at  = excluded.updated_at",
                rusqlite::params![
                    t.slug,
                    local_project_id.0,
                    t.title,
                    i64::from(t.done),
                    t.archived_at.map(|dt| dt.to_rfc3339()),
                    t.created_at.to_rfc3339(),
                    t.updated_at.to_rfc3339(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn store() -> SqliteTodos {
        let conn = open_in_memory().expect("in-memory db");
        SqliteTodos::new(Arc::new(Mutex::new(conn)))
    }

    fn new_todo(slug: &str, title: &str) -> NewTodo {
        NewTodo {
            slug: slug.to_owned(),
            project_id: ProjectId(1),
            title: title.to_owned(),
        }
    }

    #[test]
    fn test_create_and_find() {
        let s = store();
        let t = s.create(new_todo("t1", "Do thing")).expect("create");
        assert_eq!(t.slug, "t1");
        assert!(!t.done);
    }

    #[test]
    fn test_mark_done() {
        let s = store();
        s.create(new_todo("td", "Do it")).expect("create");
        let updated = s
            .update(
                "td",
                TodoPatch {
                    done: Some(true),
                    ..Default::default()
                },
            )
            .expect("update");
        assert!(updated.done);
    }

    #[test]
    fn test_archive_hide_from_list() {
        let s = store();
        s.create(new_todo("ta", "Archive me")).expect("create");
        s.archive("ta").expect("archive");
        let items = s.list(None, true, false).expect("list");
        assert!(!items.iter().any(|t| t.slug == "ta"));
    }
}
