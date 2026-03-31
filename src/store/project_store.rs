// Rust guideline compliant 2026-02-21
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

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn store() -> SqliteProjects {
        let conn = open_in_memory().expect("in-memory db");
        SqliteProjects::new(Arc::new(Mutex::new(conn)))
    }

    fn new_project(slug: &str, name: &str) -> NewProject {
        NewProject {
            slug: slug.to_owned(),
            name: name.to_owned(),
            description: None,
            status: ProjectStatus::Active,
        }
    }

    #[test]
    fn test_create_and_find_by_slug() {
        let s = store();
        let p = s.create(new_project("alpha", "Alpha")).expect("create");
        assert_eq!(p.slug, "alpha");
        let found = s.find_by_slug("alpha").expect("find").expect("some");
        assert_eq!(found.id, p.id);
    }

    #[test]
    fn test_list_active_excludes_archived() {
        let s = store();
        s.create(new_project("p1", "P1")).expect("p1");
        s.create(new_project("p2", "P2")).expect("p2");
        s.archive("p1").expect("archive");
        let active = s.list_active().expect("list");
        assert!(active.iter().any(|p| p.slug == "p2"));
        assert!(!active.iter().any(|p| p.slug == "p1"));
    }

    #[test]
    fn test_archive_blocked_on_reserved() {
        let s = store();
        let err = s.archive("quick-capture").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn test_delete_blocked_on_reserved() {
        let s = store();
        let err = s.delete("quick-capture").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn test_restore_clears_archived_at() {
        let s = store();
        s.create(new_project("r1", "R1")).expect("create");
        s.archive("r1").expect("archive");
        let restored = s.restore("r1").expect("restore");
        assert!(restored.archived_at.is_none());
    }

    #[test]
    fn test_update_name() {
        let s = store();
        s.create(new_project("upd", "Old Name")).expect("create");
        let updated = s
            .update(
                "upd",
                ProjectPatch {
                    name: Some("New Name".to_owned()),
                    ..Default::default()
                },
            )
            .expect("update");
        assert_eq!(updated.name, "New Name");
    }

    #[test]
    fn test_list_filtered_by_status() {
        let s = store();
        s.create(NewProject {
            status: ProjectStatus::Paused,
            ..new_project("paused-one", "Paused One")
        })
        .expect("create paused");
        let paused = s
            .list(Some(ProjectStatus::Paused), false)
            .expect("list paused");
        assert!(paused.iter().all(|p| p.status == ProjectStatus::Paused));
        assert!(paused.iter().any(|p| p.slug == "paused-one"));
    }
}
