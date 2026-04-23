//! `SQLite` implementation of the [`Notes`] and [`Links`] repository traits.
//!
//! [`SqliteNotes`] provides full CRUD for the `notes` table.
//! [`SqliteLinks`] provides bi-directional link management for the `links` table.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::domain::{Link, Links, NewNote, Note, NoteId, NotePatch, Notes};
use crate::store::project_store::parse_dt;

// ── row mapping ─────────────────────────────────────────────────────────────

const SELECT_COLS: &str = "id, slug, title, content, created_at, updated_at";

struct RawNoteRow {
    id: i64,
    slug: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
}

impl RawNoteRow {
    fn into_note(self) -> anyhow::Result<Note> {
        Ok(Note {
            id: NoteId(self.id),
            slug: self.slug,
            title: self.title,
            content: self.content,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
        })
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawNoteRow> {
    Ok(RawNoteRow {
        id: row.get(0)?,
        slug: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<Note>> {
    let sql = format!("SELECT {SELECT_COLS} FROM notes WHERE slug = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut iter = stmt.query_map(params![slug], map_row)?;
    iter.next()
        .transpose()
        .map_err(anyhow::Error::from)?
        .map(RawNoteRow::into_note)
        .transpose()
}

// ── SqliteNotes ─────────────────────────────────────────────────────────────

/// `SQLite`-backed implementation of the [`Notes`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
#[derive(Clone, Debug)]
pub struct SqliteNotes {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteNotes {
    /// Creates a new [`SqliteNotes`] wrapping the given shared connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteNotes;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteNotes::new(conn);
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

impl Notes for SqliteNotes {
    fn create(&self, note: NewNote) -> anyhow::Result<Note> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO notes (slug, title, content, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![note.slug, note.title, note.content, now, now],
        )?;
        fetch_one(&conn, &note.slug)?
            .ok_or_else(|| anyhow::anyhow!("note '{}' not found after insert", note.slug))
    }

    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Note>> {
        let conn = self.lock()?;
        fetch_one(&conn, slug)
    }

    fn list(&self) -> anyhow::Result<Vec<Note>> {
        let conn = self.lock()?;
        let sql = format!("SELECT {SELECT_COLS} FROM notes ORDER BY created_at");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_note())
            .collect()
    }

    fn update(&self, slug: &str, patch: NotePatch) -> anyhow::Result<Note> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();

        let mut sets: Vec<String> = vec!["updated_at = ?1".to_owned()];
        let mut extra: Vec<Option<String>> = Vec::new();

        if let Some(ref v) = patch.title {
            let i = extra.len() + 2;
            sets.push(format!("title = ?{i}"));
            extra.push(Some(v.clone()));
        }
        if let Some(ref v) = patch.content {
            let i = extra.len() + 2;
            sets.push(format!("content = ?{i}"));
            extra.push(Some(v.clone()));
        }

        let where_i = extra.len() + 2;
        let sql = format!(
            "UPDATE notes SET {} WHERE slug = ?{where_i}",
            sets.join(", ")
        );

        let mut all_params: Vec<Option<String>> = vec![Some(now)];
        all_params.extend(extra);
        all_params.push(Some(slug.to_owned()));

        let sql_params: Vec<&dyn rusqlite::ToSql> = all_params
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();
        let rows = conn.execute(&sql, sql_params.as_slice())?;
        if rows == 0 {
            return Err(anyhow::anyhow!("note '{slug}' not found"));
        }
        fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("note '{slug}' not found after update"))
    }

    fn delete(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let rows = conn.execute("DELETE FROM notes WHERE slug = ?1", params![slug])?;
        if rows == 0 {
            return Err(anyhow::anyhow!("note '{slug}' not found"));
        }
        Ok(())
    }
}

// ── SqliteLinks ─────────────────────────────────────────────────────────────

/// `SQLite`-backed implementation of the [`Links`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
#[derive(Clone, Debug)]
pub struct SqliteLinks {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteLinks {
    /// Creates a new [`SqliteLinks`] wrapping the given shared connection.
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

impl Links for SqliteLinks {
    fn create(&self, source_slug: &str, target_slug: &str) -> anyhow::Result<Link> {
        let conn = self.lock()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO links (source_slug, target_slug, created_at) \
             VALUES (?1, ?2, ?3)",
            params![source_slug, target_slug, now],
        )?;
        let rows: Vec<Link> = conn
            .prepare(
                "SELECT id, source_slug, target_slug, created_at \
                 FROM links WHERE source_slug = ?1 AND target_slug = ?2",
            )?
            .query_map(params![source_slug, target_slug], map_link_row)?
            .map(|r| r.map_err(anyhow::Error::from))
            .collect::<anyhow::Result<_>>()?;
        rows.into_iter().next().ok_or_else(|| {
            anyhow::anyhow!(
                "link not found after insert (source={source_slug}, target={target_slug})"
            )
        })
    }

    fn outbound_for(&self, slug: &str) -> anyhow::Result<Vec<Link>> {
        let conn = self.lock()?;
        let sql = "SELECT id, source_slug, target_slug, created_at \
                   FROM links WHERE source_slug = ?1 ORDER BY created_at";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![slug], map_link_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn inbound_for(&self, slug: &str) -> anyhow::Result<Vec<Link>> {
        let conn = self.lock()?;
        let sql = "SELECT id, source_slug, target_slug, created_at \
                   FROM links WHERE target_slug = ?1 ORDER BY created_at";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![slug], map_link_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)).collect()
    }

    fn delete(&self, source_slug: &str, target_slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let rows = conn.execute(
            "DELETE FROM links WHERE source_slug = ?1 AND target_slug = ?2",
            params![source_slug, target_slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!(
                "link not found (source={source_slug}, target={target_slug})"
            ));
        }
        Ok(())
    }

    fn delete_all_for(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM links WHERE source_slug = ?1 OR target_slug = ?1",
            params![slug],
        )?;
        Ok(())
    }
}

fn map_link_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Link> {
    let created_at_str: String = row.get(3)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&created_at_str, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .map_err(|_e| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid timestamp '{created_at_str}'"),
                )),
            )
        })?;
    Ok(Link {
        id: row.get(0)?,
        source_slug: row.get(1)?,
        target_slug: row.get(2)?,
        created_at,
    })
}

// ── test helpers ─────────────────────────────────────────────────────────

pub mod testing {
    //! Test helpers for the note store module.

    use super::{Arc, Mutex, SqliteLinks, SqliteNotes};
    use crate::db::open_in_memory;

    /// Constructs a [`SqliteNotes`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn notes_store() -> SqliteNotes {
        let conn = open_in_memory().expect("in-memory db");
        SqliteNotes::new(Arc::new(Mutex::new(conn)))
    }

    /// Constructs a [`SqliteLinks`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn links_store() -> SqliteLinks {
        let conn = open_in_memory().expect("in-memory db");
        SqliteLinks::new(Arc::new(Mutex::new(conn)))
    }
}
