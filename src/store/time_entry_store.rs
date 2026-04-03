// Rust guideline compliant 2026-02-21
//! `SQLite` implementation of the [`TimeEntries`] repository trait.
//!
//! Wired into the CLI via [`crate::ops::TrackerOps`].

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::domain::{
    NewTimeEntry, ProjectId, TaskId, TimeEntries, TimeEntry, TimeEntryId, TimeEntryPatch,
};
use crate::store::project_store::{parse_dt, parse_dt_opt};

const SELECT_COLS: &str = "id, slug, project_id, task_id, started_at, ended_at, \
     note, archived_at, created_at";

struct RawRow {
    id: i64,
    slug: String,
    project_id: i64,
    task_id: Option<i64>,
    started_at: String,
    ended_at: Option<String>,
    note: Option<String>,
    archived_at: Option<String>,
    created_at: String,
}

struct RawRowWithSlugs {
    raw: RawRow,
    project_slug: String,
    task_slug: Option<String>,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        slug: row.get(1)?,
        project_id: row.get(2)?,
        task_id: row.get(3)?,
        started_at: row.get(4)?,
        ended_at: row.get(5)?,
        note: row.get(6)?,
        archived_at: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn map_row_with_slugs(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRowWithSlugs> {
    Ok(RawRowWithSlugs {
        raw: RawRow {
            id: row.get(0)?,
            slug: row.get(1)?,
            project_id: row.get(2)?,
            task_id: row.get(3)?,
            started_at: row.get(6)?,
            ended_at: row.get(7)?,
            note: row.get(8)?,
            archived_at: row.get(9)?,
            created_at: row.get(10)?,
        },
        project_slug: row.get(4)?,
        task_slug: row.get(5)?,
    })
}

impl RawRow {
    fn into_entry(
        self,
        project_slug: Option<String>,
        task_slug: Option<String>,
    ) -> anyhow::Result<TimeEntry> {
        let project_slug = project_slug.unwrap_or_else(|| "unknown".to_owned());
        Ok(TimeEntry {
            id: TimeEntryId(self.id),
            slug: self.slug,
            project_id: ProjectId(self.project_id),
            project_slug,
            task_id: self.task_id.map(TaskId),
            task_slug,
            started_at: parse_dt(&self.started_at)?,
            ended_at: parse_dt_opt(self.ended_at)?,
            note: self.note,
            archived_at: parse_dt_opt(self.archived_at)?,
            created_at: parse_dt(&self.created_at)?,
        })
    }
}

/// `SQLite`-backed implementation of the [`TimeEntries`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
#[derive(Clone, Debug)]
pub struct SqliteTimeEntries {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteTimeEntries {
    /// Creates a new [`SqliteTimeEntries`] wrapping the given shared connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteTimeEntries;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteTimeEntries::new(conn);
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

    fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<TimeEntry>> {
        let sql = "SELECT e.id, e.slug, e.project_id, e.task_id, p.slug, t.slug, \
             e.started_at, e.ended_at, e.note, e.archived_at, e.created_at \
             FROM time_entries e \
             JOIN projects p ON e.project_id = p.id \
             LEFT JOIN tasks t ON e.task_id = t.id \
             WHERE e.slug = ?1";
        let mut stmt = conn.prepare(sql)?;
        let mut iter = stmt.query_map(params![slug], map_row_with_slugs)?;
        iter.next()
            .transpose()
            .map_err(anyhow::Error::from)?
            .map(|r| {
                let RawRowWithSlugs {
                    raw,
                    project_slug,
                    task_slug,
                } = r;
                raw.into_entry(Some(project_slug), task_slug)
            })
            .transpose()
    }
}

impl TimeEntries for SqliteTimeEntries {
    fn create(&self, entry: NewTimeEntry) -> anyhow::Result<TimeEntry> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO time_entries \
             (slug, project_id, task_id, started_at, note, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.slug,
                entry.project_id.0,
                entry.task_id.map(|t| t.0),
                entry.started_at.to_rfc3339(),
                entry.note,
                now,
            ],
        )?;
        Self::fetch_one(&conn, &entry.slug)?
            .ok_or_else(|| anyhow::anyhow!("entry '{}' not found after insert", entry.slug))
    }

    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<TimeEntry>> {
        let conn = self.lock()?;
        Self::fetch_one(&conn, slug)
    }

    fn find_running(&self) -> anyhow::Result<Option<TimeEntry>> {
        let conn = self.lock()?;
        let sql = format!("SELECT {SELECT_COLS} FROM time_entries WHERE ended_at IS NULL LIMIT 1");
        let mut stmt = conn.prepare(&sql)?;
        let mut iter = stmt.query_map([], map_row)?;
        iter.next()
            .transpose()
            .map_err(anyhow::Error::from)?
            .map(|r| r.into_entry(None, None))
            .transpose()
    }

    fn list(
        &self,
        project_id: Option<ProjectId>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<TimeEntry>> {
        let conn = self.lock()?;
        let mut conditions: Vec<String> = Vec::new();
        if !include_archived {
            conditions.push("archived_at IS NULL".to_owned());
        }
        if let Some(pid) = project_id {
            conditions.push(format!("project_id = {}", pid.0));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT {SELECT_COLS} FROM time_entries {where_clause} ORDER BY started_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_entry(None, None))
            .collect()
    }

    fn stop(&self, slug: &str, ended_at: DateTime<Utc>) -> anyhow::Result<TimeEntry> {
        let conn = self.lock()?;
        let rows = conn.execute(
            "UPDATE time_entries SET ended_at = ?1 WHERE slug = ?2",
            params![ended_at.to_rfc3339(), slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("time entry '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("entry '{slug}' not found after stop"))
    }

    fn archive(&self, slug: &str) -> anyhow::Result<TimeEntry> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE time_entries SET archived_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("time entry '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("entry '{slug}' not found after archive"))
    }

    fn restore(&self, slug: &str) -> anyhow::Result<TimeEntry> {
        let conn = self.lock()?;
        let rows = conn.execute(
            "UPDATE time_entries SET archived_at = NULL WHERE slug = ?1",
            params![slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("time entry '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("entry '{slug}' not found after restore"))
    }

    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE time_entries SET archived_at = ?1 \
             WHERE project_id = ?2 AND archived_at IS NULL",
            params![now, project_id.0],
        )?;
        Ok(())
    }

    fn delete(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let rows = conn.execute("DELETE FROM time_entries WHERE slug = ?1", params![slug])?;
        if rows == 0 {
            return Err(anyhow::anyhow!("time entry '{slug}' not found"));
        }
        Ok(())
    }

    fn update(&self, slug: &str, patch: TimeEntryPatch) -> anyhow::Result<TimeEntry> {
        let conn = self.lock()?;
        // Only the note field is mutable via the patch for now.
        let rows = conn.execute(
            "UPDATE time_entries SET note = ?1 WHERE slug = ?2",
            params![patch.note, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("time entry '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("entry '{slug}' not found after update"))
    }

    fn list_completed_in_range(
        &self,
        project_id: Option<ProjectId>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> anyhow::Result<Vec<TimeEntry>> {
        let conn = self.lock()?;
        let mut conditions: Vec<String> = vec![
            "ended_at IS NOT NULL".to_owned(),
            "archived_at IS NULL".to_owned(),
            format!("started_at >= '{}'", since.to_rfc3339()),
            format!("started_at < '{}'", until.to_rfc3339()),
        ];
        if let Some(pid) = project_id {
            conditions.push(format!("project_id = {}", pid.0));
        }
        let where_clause = format!("WHERE {}", conditions.join(" AND "));
        let sql =
            format!("SELECT {SELECT_COLS} FROM time_entries {where_clause} ORDER BY started_at");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_entry(None, None))
            .collect()
    }
}

#[cfg(feature = "sync")]
impl SqliteTimeEntries {
    /// Returns every time entry row, including archived ones.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_all(&self) -> anyhow::Result<Vec<TimeEntry>> {
        let conn = self.lock()?;
        let sql = "SELECT e.id, e.slug, e.project_id, e.task_id, p.slug, t.slug, \
             e.started_at, e.ended_at, e.note, e.archived_at, e.created_at \
             FROM time_entries e \
             JOIN projects p ON e.project_id = p.id \
             LEFT JOIN tasks t ON e.task_id = t.id \
             ORDER BY e.created_at ASC";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], map_row_with_slugs)?;
        rows.map(|r| {
            let RawRowWithSlugs {
                raw,
                project_slug,
                task_slug,
            } = r.map_err(anyhow::Error::from)?;
            raw.into_entry(Some(project_slug), task_slug)
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

    fn resolve_task_id(conn: &Connection, task_slug: &str) -> anyhow::Result<TaskId> {
        let id = conn
            .query_row(
                "SELECT id FROM tasks WHERE slug = ?1",
                params![task_slug],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| anyhow::anyhow!("task '{task_slug}' not found: {e}"))?;
        Ok(TaskId(id))
    }

    /// Inserts or updates each time entry by slug, resolving project and task slugs to local IDs.
    ///
    /// This is the sync-safe version. It resolves `project_slug` and `task_slug`
    /// to local numeric IDs before inserting, avoiding foreign key mismatches.
    ///
    /// # Errors
    ///
    /// Returns an error if any project or task slug cannot be resolved or if
    /// any database write fails.
    pub fn upsert_all_with_slug_resolution(&self, entries: &[TimeEntry]) -> anyhow::Result<()> {
        let mut conn = self.lock()?;

        let entry_data: Vec<_> = entries
            .iter()
            .map(|e| {
                let local_project_id = Self::resolve_project_id(&conn, &e.project_slug)?;
                let local_task_id = if let Some(ref task_slug) = e.task_slug {
                    Some(Self::resolve_task_id(&conn, task_slug)?)
                } else {
                    None
                };
                Ok((local_project_id, local_task_id, e))
            })
            .collect::<anyhow::Result<_>>()?;

        let tx = conn.transaction()?;
        for (local_project_id, local_task_id, e) in entry_data {
            tx.execute(
                "INSERT INTO time_entries \
                 (slug, project_id, task_id, started_at, ended_at, note, archived_at, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(slug) DO UPDATE SET \
                   started_at  = excluded.started_at, \
                   ended_at    = excluded.ended_at, \
                   note        = excluded.note, \
                   archived_at = excluded.archived_at",
                rusqlite::params![
                    e.slug,
                    local_project_id.0,
                    local_task_id.map(|t| t.0),
                    e.started_at.to_rfc3339(),
                    e.ended_at.map(|dt| dt.to_rfc3339()),
                    e.note,
                    e.archived_at.map(|dt| dt.to_rfc3339()),
                    e.created_at.to_rfc3339(),
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

    fn store() -> SqliteTimeEntries {
        let conn = open_in_memory().expect("in-memory db");
        SqliteTimeEntries::new(Arc::new(Mutex::new(conn)))
    }

    fn new_entry(slug: &str) -> NewTimeEntry {
        NewTimeEntry {
            slug: slug.to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            started_at: Utc::now(),
            note: None,
        }
    }

    #[test]
    fn test_create_find_running() {
        let s = store();
        s.create(new_entry("e1")).expect("create");
        let running = s.find_running().expect("find running").expect("some");
        assert_eq!(running.slug, "e1");
    }

    #[test]
    fn test_stop() {
        let s = store();
        s.create(new_entry("e2")).expect("create");
        let stopped = s.stop("e2", Utc::now()).expect("stop");
        assert!(stopped.ended_at.is_some());
        assert!(s.find_running().expect("running").is_none());
    }
}
