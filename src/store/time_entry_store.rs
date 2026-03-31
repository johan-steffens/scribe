// Rust guideline compliant 2026-02-21
//! `SQLite` implementation of the [`TimeEntries`] repository trait.
//!
//! Phase 2+: this store is not yet wired into the CLI binary.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::domain::{NewTimeEntry, ProjectId, TaskId, TimeEntries, TimeEntry, TimeEntryId};
use crate::store::project_store::{parse_dt, parse_dt_opt};

// Phase 2+: items below unused in the binary until Phase 2.
#[allow(dead_code, reason = "used in Phase 2 time tracking feature")]
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

impl RawRow {
    fn into_entry(self) -> anyhow::Result<TimeEntry> {
        Ok(TimeEntry {
            id: TimeEntryId(self.id),
            slug: self.slug,
            project_id: ProjectId(self.project_id),
            task_id: self.task_id.map(TaskId),
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
// Phase 2+: not yet constructed in the CLI binary.
#[allow(dead_code, reason = "used in Phase 2 time tracking feature")]
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
        let sql = format!("SELECT {SELECT_COLS} FROM time_entries WHERE slug = ?1");
        let mut stmt = conn.prepare(&sql)?;
        let mut iter = stmt.query_map(params![slug], map_row)?;
        iter.next()
            .transpose()
            .map_err(anyhow::Error::from)?
            .map(RawRow::into_entry)
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
            .map(RawRow::into_entry)
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
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_entry())
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
