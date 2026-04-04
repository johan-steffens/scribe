//! `SQLite` implementation of the [`Projects`] repository trait.
//!
//! [`SqliteProjects`] wraps a shared `Arc<Mutex<Connection>>` and provides
//! full CRUD plus archive/restore operations for the `projects` table.
//! All timestamps are stored and retrieved as ISO 8601 strings.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::types::ToSql;
use rusqlite::{Connection, params};

use crate::domain::{NewProject, Project, ProjectId, ProjectPatch, ProjectStatus, Projects};

// ── timestamp parsing ──────────────────────────────────────────────────────

/// Parses an ISO 8601 (RFC 3339 or `SQLite`) timestamp string into `DateTime<Utc>`.
pub(crate) fn parse_dt(s: &str) -> anyhow::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            // Fallback for SQLite's `datetime()` output format
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map(|ndt| ndt.and_utc())
        })
        .map_err(|e| anyhow::anyhow!("invalid timestamp '{s}': {e}"))
}

/// Parses an optional timestamp string.
pub(crate) fn parse_dt_opt(s: Option<String>) -> anyhow::Result<Option<DateTime<Utc>>> {
    s.map(|v| parse_dt(&v)).transpose()
}

// ── row mapping ────────────────────────────────────────────────────────────

const SELECT_COLS: &str =
    "id, slug, name, description, status, is_reserved, archived_at, created_at, updated_at";

struct RawRow {
    id: i64,
    slug: String,
    name: String,
    description: Option<String>,
    status: String,
    is_reserved: bool,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RawRow {
    fn into_project(self) -> anyhow::Result<Project> {
        Ok(Project {
            id: ProjectId(self.id),
            slug: self.slug,
            name: self.name,
            description: self.description,
            status: self
                .status
                .parse::<ProjectStatus>()
                .map_err(|e| anyhow::anyhow!(e))?,
            is_reserved: self.is_reserved,
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
        name: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        is_reserved: row.get::<_, i64>(5)? != 0,
        archived_at: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<Project>> {
    let sql = format!("SELECT {SELECT_COLS} FROM projects WHERE slug = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut iter = stmt.query_map(params![slug], map_row)?;
    iter.next()
        .transpose()
        .map_err(anyhow::Error::from)?
        .map(RawRow::into_project)
        .transpose()
}

// ── SqliteProjects ─────────────────────────────────────────────────────────

/// `SQLite`-backed implementation of the [`Projects`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
#[derive(Clone, Debug)]
pub struct SqliteProjects {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteProjects {
    /// Creates a new [`SqliteProjects`] wrapping the given shared connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteProjects;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteProjects::new(conn);
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

impl Projects for SqliteProjects {
    fn create(&self, project: NewProject) -> anyhow::Result<Project> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO projects \
             (slug, name, description, status, is_reserved, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
            params![
                project.slug,
                project.name,
                project.description,
                project.status.to_string(),
                now,
            ],
        )?;
        fetch_one(&conn, &project.slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{}' not found after insert", project.slug))
    }

    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Project>> {
        let conn = self.lock()?;
        fetch_one(&conn, slug)
    }

    fn list_active(&self) -> anyhow::Result<Vec<Project>> {
        let conn = self.lock()?;
        let sql = format!(
            "SELECT {SELECT_COLS} FROM projects \
             WHERE archived_at IS NULL ORDER BY created_at"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_project())
            .collect()
    }

    fn list_archived(&self) -> anyhow::Result<Vec<Project>> {
        let conn = self.lock()?;
        let sql = format!(
            "SELECT {SELECT_COLS} FROM projects \
             WHERE archived_at IS NOT NULL ORDER BY archived_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_project())
            .collect()
    }

    fn list(
        &self,
        status: Option<ProjectStatus>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Project>> {
        let conn = self.lock()?;
        let mut conditions: Vec<String> = Vec::new();
        if !include_archived {
            conditions.push("archived_at IS NULL".to_owned());
        }
        if let Some(s) = &status {
            conditions.push(format!("status = '{s}'"));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!("SELECT {SELECT_COLS} FROM projects {where_clause} ORDER BY created_at");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_project())
            .collect()
    }

    fn update(&self, slug: &str, patch: ProjectPatch) -> anyhow::Result<Project> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();

        // Build SET clause and params list dynamically.
        let mut sets: Vec<String> = vec!["updated_at = ?1".to_owned()];
        // Index 0 = now; remaining are patch values; last = slug for WHERE.
        let mut extra: Vec<Option<String>> = Vec::new();

        if let Some(ref v) = patch.slug {
            let i = extra.len() + 2;
            sets.push(format!("slug = ?{i}"));
            extra.push(Some(v.clone()));
        }
        if let Some(ref v) = patch.name {
            let i = extra.len() + 2;
            sets.push(format!("name = ?{i}"));
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

        let where_i = extra.len() + 2;
        let sql = format!(
            "UPDATE projects SET {} WHERE slug = ?{where_i}",
            sets.join(", ")
        );

        // Build the final params vec: [now, ...extra, slug].
        let mut all_params: Vec<Option<String>> = vec![Some(now)];
        all_params.extend(extra);
        all_params.push(Some(slug.to_owned()));

        let sql_params: Vec<&dyn ToSql> = all_params.iter().map(|v| v as &dyn ToSql).collect();
        let rows = conn.execute(&sql, sql_params.as_slice())?;
        if rows == 0 {
            return Err(anyhow::anyhow!("project '{slug}' not found"));
        }

        let effective_slug = patch.slug.as_deref().unwrap_or(slug);
        fetch_one(&conn, effective_slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{effective_slug}' not found after update"))
    }

    fn archive(&self, slug: &str) -> anyhow::Result<Project> {
        let conn = self.lock()?;
        let is_reserved: i64 = conn
            .query_row(
                "SELECT is_reserved FROM projects WHERE slug = ?1",
                params![slug],
                |r| r.get(0),
            )
            .map_err(|e| anyhow::anyhow!("project '{slug}' not found: {e}"))?;

        if is_reserved != 0 {
            return Err(anyhow::anyhow!(
                "project '{slug}' is reserved and cannot be archived"
            ));
        }

        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE projects SET archived_at = ?1, updated_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{slug}' not found after archive"))
    }

    fn restore(&self, slug: &str) -> anyhow::Result<Project> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE projects SET archived_at = NULL, updated_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("project '{slug}' not found"));
        }
        fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("project '{slug}' not found after restore"))
    }

    fn delete(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let is_reserved: Option<i64> = conn
            .query_row(
                "SELECT is_reserved FROM projects WHERE slug = ?1",
                params![slug],
                |r| r.get(0),
            )
            .ok();

        match is_reserved {
            None => return Err(anyhow::anyhow!("project '{slug}' not found")),
            Some(1) => {
                return Err(anyhow::anyhow!(
                    "project '{slug}' is reserved and cannot be deleted"
                ));
            }
            Some(_) => {}
        }

        conn.execute("DELETE FROM projects WHERE slug = ?1", params![slug])?;
        Ok(())
    }
}

#[cfg(feature = "sync")]
impl SqliteProjects {
    /// Returns every project row, including archived ones.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_all(&self) -> anyhow::Result<Vec<Project>> {
        let conn = self.lock()?;
        let sql = format!("SELECT {SELECT_COLS} FROM projects ORDER BY created_at ASC");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_project())
            .collect()
    }

    /// Inserts or updates each project by slug.
    ///
    /// If a project with the same slug already exists, all mutable fields are
    /// updated. `is_reserved` and `created_at` are intentionally excluded from
    /// the update — they are write-once fields.
    ///
    /// # Errors
    ///
    /// Returns an error if any database write fails.
    pub fn upsert_all(&self, projects: &[Project]) -> anyhow::Result<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        for p in projects {
            tx.execute(
                "INSERT INTO projects \
                 (slug, name, description, status, is_reserved, archived_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(slug) DO UPDATE SET \
                   name        = excluded.name, \
                   description = excluded.description, \
                   status      = excluded.status, \
                   archived_at = excluded.archived_at, \
                   updated_at  = excluded.updated_at",
                rusqlite::params![
                    p.slug,
                    p.name,
                    p.description,
                    p.status.to_string(),
                    i64::from(p.is_reserved),
                    p.archived_at.map(|t| t.to_rfc3339()),
                    p.created_at.to_rfc3339(),
                    p.updated_at.to_rfc3339(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

// ── test helpers ─────────────────────────────────────────────────────────

#[cfg(feature = "test-util")]
pub mod testing {
    //! Test helpers for the project store module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::SqliteProjects`] instances against an in-memory database.

    use super::{Arc, Mutex, SqliteProjects};
    use crate::db::open_in_memory;

    /// Constructs a [`SqliteProjects`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn store() -> SqliteProjects {
        let conn = open_in_memory().expect("in-memory db");
        SqliteProjects::new(Arc::new(Mutex::new(conn)))
    }
}
