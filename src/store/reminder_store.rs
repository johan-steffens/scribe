// Rust guideline compliant 2026-02-21
//! `SQLite` implementation of the [`Reminders`] repository trait.
//!
//! Wired into the CLI via [`crate::ops::ReminderOps`].

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::domain::{
    NewReminder, ProjectId, Reminder, ReminderId, ReminderPatch, Reminders, TaskId,
};
use crate::store::project_store::{parse_dt, parse_dt_opt};

const SELECT_COLS: &str =
    "id, slug, project_id, task_id, remind_at, message, fired, persistent, archived_at, created_at";

struct RawRow {
    id: i64,
    slug: String,
    project_id: i64,
    task_id: Option<i64>,
    remind_at: String,
    message: Option<String>,
    fired: bool,
    persistent: bool,
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
        remind_at: row.get(4)?,
        message: row.get(5)?,
        fired: row.get::<_, i64>(6)? != 0,
        persistent: row.get::<_, i64>(7)? != 0,
        archived_at: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn map_row_with_slugs(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRowWithSlugs> {
    Ok(RawRowWithSlugs {
        raw: RawRow {
            id: row.get(0)?,
            slug: row.get(1)?,
            project_id: row.get(2)?,
            task_id: row.get(3)?,
            remind_at: row.get(6)?,
            message: row.get(7)?,
            fired: row.get::<_, i64>(8)? != 0,
            persistent: row.get::<_, i64>(9)? != 0,
            archived_at: row.get(10)?,
            created_at: row.get(11)?,
        },
        project_slug: row.get(4)?,
        task_slug: row.get(5)?,
    })
}

impl RawRow {
    fn into_reminder(
        self,
        project_slug: Option<String>,
        task_slug: Option<String>,
    ) -> anyhow::Result<Reminder> {
        let project_slug = project_slug.unwrap_or_else(|| "unknown".to_owned());
        Ok(Reminder {
            id: ReminderId(self.id),
            slug: self.slug,
            project_id: ProjectId(self.project_id),
            project_slug,
            task_id: self.task_id.map(TaskId),
            task_slug,
            remind_at: parse_dt(&self.remind_at)?,
            message: self.message,
            fired: self.fired,
            persistent: self.persistent,
            archived_at: parse_dt_opt(self.archived_at)?,
            created_at: parse_dt(&self.created_at)?,
        })
    }
}

/// `SQLite`-backed implementation of the [`Reminders`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
#[derive(Clone, Debug)]
pub struct SqliteReminders {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteReminders {
    /// Creates a new [`SqliteReminders`] wrapping the given shared connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteReminders;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteReminders::new(conn);
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

    fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<Reminder>> {
        let sql = "SELECT r.id, r.slug, r.project_id, r.task_id, p.slug, t.slug, \
             r.remind_at, r.message, r.fired, r.persistent, r.archived_at, r.created_at \
             FROM reminders r \
             JOIN projects p ON r.project_id = p.id \
             LEFT JOIN tasks t ON r.task_id = t.id \
             WHERE r.slug = ?1";
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
                raw.into_reminder(Some(project_slug), task_slug)
            })
            .transpose()
    }
}

impl Reminders for SqliteReminders {
    fn create(&self, reminder: NewReminder) -> anyhow::Result<Reminder> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO reminders \
             (slug, project_id, task_id, remind_at, message, fired, persistent, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)",
            params![
                reminder.slug,
                reminder.project_id.0,
                reminder.task_id.map(|t| t.0),
                reminder.remind_at.to_rfc3339(),
                reminder.message,
                i64::from(reminder.persistent),
                now,
            ],
        )?;
        Self::fetch_one(&conn, &reminder.slug)?
            .ok_or_else(|| anyhow::anyhow!("reminder '{}' not found after insert", reminder.slug))
    }

    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Reminder>> {
        let conn = self.lock()?;
        Self::fetch_one(&conn, slug)
    }

    fn list(
        &self,
        project_id: Option<ProjectId>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Reminder>> {
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
        let sql = format!("SELECT {SELECT_COLS} FROM reminders {where_clause} ORDER BY remind_at");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_reminder(None, None))
            .collect()
    }

    fn archive(&self, slug: &str) -> anyhow::Result<Reminder> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE reminders SET archived_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("reminder '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("reminder '{slug}' not found after archive"))
    }

    fn restore(&self, slug: &str) -> anyhow::Result<Reminder> {
        let conn = self.lock()?;
        let rows = conn.execute(
            "UPDATE reminders SET archived_at = NULL WHERE slug = ?1",
            params![slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("reminder '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("reminder '{slug}' not found after restore"))
    }

    fn delete(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let rows = conn.execute("DELETE FROM reminders WHERE slug = ?1", params![slug])?;
        if rows == 0 {
            return Err(anyhow::anyhow!("reminder '{slug}' not found"));
        }
        Ok(())
    }

    fn archive_all_for_project(&self, project_id: ProjectId) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE reminders SET archived_at = ?1 \
             WHERE project_id = ?2 AND archived_at IS NULL",
            params![now, project_id.0],
        )?;
        Ok(())
    }

    fn update(&self, slug: &str, patch: ReminderPatch) -> anyhow::Result<Reminder> {
        let conn = self.lock()?;
        let mut sets: Vec<String> = Vec::new();
        let mut values: Vec<String> = Vec::new();

        if let Some(ref dt) = patch.remind_at {
            sets.push(format!("remind_at = ?{}", sets.len() + 1));
            values.push(dt.to_rfc3339());
        }
        if let Some(ref msg) = patch.message {
            sets.push(format!("message = ?{}", sets.len() + 1));
            values.push(msg.clone());
        }
        if let Some(p) = patch.persistent {
            sets.push(format!("persistent = ?{}", sets.len() + 1));
            values.push(i64::from(p).to_string());
        }

        if sets.is_empty() {
            // Nothing to update — reload and return.
            return Self::fetch_one(&conn, slug)?
                .ok_or_else(|| anyhow::anyhow!("reminder '{slug}' not found"));
        }

        let where_i = sets.len() + 1;
        let sql = format!(
            "UPDATE reminders SET {} WHERE slug = ?{where_i}",
            sets.join(", ")
        );
        let mut all: Vec<&dyn rusqlite::types::ToSql> = values
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        all.push(&slug);

        let rows = conn.execute(&sql, all.as_slice())?;
        if rows == 0 {
            return Err(anyhow::anyhow!("reminder '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("reminder '{slug}' not found after update"))
    }

    fn list_due(&self, before: DateTime<Utc>) -> anyhow::Result<Vec<Reminder>> {
        let conn = self.lock()?;
        let before_str = before.to_rfc3339();
        let sql = format!(
            "SELECT {SELECT_COLS} FROM reminders \
             WHERE fired = 0 AND archived_at IS NULL AND remind_at <= ?1 \
             ORDER BY remind_at"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![before_str], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_reminder(None, None))
            .collect()
    }

    fn mark_fired(&self, slug: &str) -> anyhow::Result<Reminder> {
        let conn = self.lock()?;
        let rows = conn.execute(
            "UPDATE reminders SET fired = 1 WHERE slug = ?1",
            params![slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("reminder '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("reminder '{slug}' not found after mark_fired"))
    }
}

#[cfg(feature = "sync")]
impl SqliteReminders {
    /// Returns every reminder row, including archived ones.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list_all(&self) -> anyhow::Result<Vec<Reminder>> {
        let conn = self.lock()?;
        let sql = "SELECT r.id, r.slug, r.project_id, r.task_id, p.slug, t.slug, \
             r.remind_at, r.message, r.fired, r.persistent, r.archived_at, r.created_at \
             FROM reminders r \
             JOIN projects p ON r.project_id = p.id \
             LEFT JOIN tasks t ON r.task_id = t.id \
             ORDER BY r.created_at ASC";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], map_row_with_slugs)?;
        rows.map(|r| {
            let RawRowWithSlugs {
                raw,
                project_slug,
                task_slug,
            } = r.map_err(anyhow::Error::from)?;
            raw.into_reminder(Some(project_slug), task_slug)
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

    /// Inserts or updates each reminder by slug, resolving project and task slugs to local IDs.
    ///
    /// This is the sync-safe version. It resolves `project_slug` and `task_slug`
    /// to local numeric IDs before inserting, avoiding foreign key mismatches.
    ///
    /// # Errors
    ///
    /// Returns an error if any project or task slug cannot be resolved or if
    /// any database write fails.
    pub fn upsert_all_with_slug_resolution(&self, reminders: &[Reminder]) -> anyhow::Result<()> {
        let mut conn = self.lock()?;

        let reminder_data: Vec<_> = reminders
            .iter()
            .map(|r| {
                let local_project_id = Self::resolve_project_id(&conn, &r.project_slug)?;
                let local_task_id = if let Some(ref task_slug) = r.task_slug {
                    Some(Self::resolve_task_id(&conn, task_slug)?)
                } else {
                    None
                };
                Ok((local_project_id, local_task_id, r))
            })
            .collect::<anyhow::Result<_>>()?;

        let tx = conn.transaction()?;
        for (local_project_id, local_task_id, r) in reminder_data {
            tx.execute(
                "INSERT INTO reminders \
                 (slug, project_id, task_id, remind_at, message, fired, persistent, \
                  archived_at, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                 ON CONFLICT(slug) DO UPDATE SET \
                   remind_at   = excluded.remind_at, \
                   message     = excluded.message, \
                   fired       = excluded.fired, \
                   persistent  = excluded.persistent, \
                   archived_at = excluded.archived_at",
                rusqlite::params![
                    r.slug,
                    local_project_id.0,
                    local_task_id.map(|t| t.0),
                    r.remind_at.to_rfc3339(),
                    r.message,
                    i64::from(r.fired),
                    i64::from(r.persistent),
                    r.archived_at.map(|dt| dt.to_rfc3339()),
                    r.created_at.to_rfc3339(),
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

    fn store() -> SqliteReminders {
        let conn = open_in_memory().expect("in-memory db");
        SqliteReminders::new(Arc::new(Mutex::new(conn)))
    }

    fn new_reminder(slug: &str) -> NewReminder {
        NewReminder {
            slug: slug.to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            remind_at: Utc::now(),
            message: Some("Reminder message".to_owned()),
            persistent: false,
        }
    }

    #[test]
    fn test_create_and_find() {
        let s = store();
        let r = s.create(new_reminder("r1")).expect("create");
        assert_eq!(r.slug, "r1");
        assert!(!r.fired);
    }

    #[test]
    fn test_archive_and_restore() {
        let s = store();
        s.create(new_reminder("r2")).expect("create");
        s.archive("r2").expect("archive");
        let items = s.list(None, false).expect("list");
        assert!(!items.iter().any(|r| r.slug == "r2"));
        s.restore("r2").expect("restore");
        let items = s.list(None, false).expect("list");
        assert!(items.iter().any(|r| r.slug == "r2"));
    }
}
