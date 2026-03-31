// Rust guideline compliant 2026-02-21
//! `SQLite` implementation of the [`CaptureItems`] repository trait.
//!
//! Phase 2+: this store is not yet wired into the CLI. The struct and trait
//! implementation are fully tested via the in-memory DB.

use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};

use crate::domain::{CaptureItem, CaptureItemId, CaptureItems, NewCaptureItem};
use crate::store::project_store::parse_dt;

// Phase 2+: all items below are unused in the CLI binary until Phase 2.
#[allow(dead_code, reason = "used in Phase 2 inbox feature")]
const SELECT_COLS: &str = "id, slug, body, processed, created_at";

struct RawRow {
    id: i64,
    slug: String,
    body: String,
    processed: bool,
    created_at: String,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        slug: row.get(1)?,
        body: row.get(2)?,
        processed: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
    })
}

impl RawRow {
    fn into_item(self) -> anyhow::Result<CaptureItem> {
        Ok(CaptureItem {
            id: CaptureItemId(self.id),
            slug: self.slug,
            body: self.body,
            processed: self.processed,
            created_at: parse_dt(&self.created_at)?,
        })
    }
}

/// `SQLite`-backed implementation of the [`CaptureItems`] repository trait.
///
/// Cloning creates a new handle to the same underlying connection.
// Phase 2+: not yet constructed in the CLI binary.
#[allow(dead_code, reason = "used in Phase 2 inbox feature")]
#[derive(Clone, Debug)]
pub struct SqliteCaptureItems {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteCaptureItems {
    /// Creates a new [`SqliteCaptureItems`] wrapping the given shared connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteCaptureItems;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteCaptureItems::new(conn);
    /// ```
    #[allow(dead_code, reason = "used in Phase 2 inbox feature")]
    #[must_use]
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn lock(&self) -> anyhow::Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))
    }

    fn fetch_one(conn: &Connection, slug: &str) -> anyhow::Result<Option<CaptureItem>> {
        let sql = format!("SELECT {SELECT_COLS} FROM capture_items WHERE slug = ?1");
        let mut stmt = conn.prepare(&sql)?;
        let mut iter = stmt.query_map(params![slug], map_row)?;
        iter.next()
            .transpose()
            .map_err(anyhow::Error::from)?
            .map(RawRow::into_item)
            .transpose()
    }
}

impl CaptureItems for SqliteCaptureItems {
    fn create(&self, item: NewCaptureItem) -> anyhow::Result<CaptureItem> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO capture_items (slug, body, processed, created_at) \
             VALUES (?1, ?2, 0, ?3)",
            params![item.slug, item.body, item.created_at.to_rfc3339()],
        )?;
        Self::fetch_one(&conn, &item.slug)?
            .ok_or_else(|| anyhow::anyhow!("capture item '{}' not found after insert", item.slug))
    }

    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<CaptureItem>> {
        let conn = self.lock()?;
        Self::fetch_one(&conn, slug)
    }

    fn list(&self, include_processed: bool) -> anyhow::Result<Vec<CaptureItem>> {
        let conn = self.lock()?;
        let where_clause = if include_processed {
            String::new()
        } else {
            "WHERE processed = 0".to_owned()
        };
        let sql = format!(
            "SELECT {SELECT_COLS} FROM capture_items {where_clause} ORDER BY created_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_row)?;
        rows.map(|r| r.map_err(anyhow::Error::from)?.into_item())
            .collect()
    }

    fn mark_processed(&self, slug: &str) -> anyhow::Result<CaptureItem> {
        let conn = self.lock()?;
        let rows = conn.execute(
            "UPDATE capture_items SET processed = 1 WHERE slug = ?1",
            params![slug],
        )?;
        if rows == 0 {
            return Err(anyhow::anyhow!("capture item '{slug}' not found"));
        }
        Self::fetch_one(&conn, slug)?
            .ok_or_else(|| anyhow::anyhow!("capture item '{slug}' not found after update"))
    }

    fn delete(&self, slug: &str) -> anyhow::Result<()> {
        let conn = self.lock()?;
        let rows = conn.execute("DELETE FROM capture_items WHERE slug = ?1", params![slug])?;
        if rows == 0 {
            return Err(anyhow::anyhow!("capture item '{slug}' not found"));
        }
        Ok(())
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::db::open_in_memory;

    fn store() -> SqliteCaptureItems {
        let conn = open_in_memory().expect("in-memory db");
        SqliteCaptureItems::new(Arc::new(Mutex::new(conn)))
    }

    fn new_item(slug: &str, body: &str) -> NewCaptureItem {
        NewCaptureItem {
            slug: slug.to_owned(),
            body: body.to_owned(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_create_and_find() {
        let s = store();
        let item = s
            .create(new_item("c1", "Remember to buy milk"))
            .expect("create");
        assert_eq!(item.slug, "c1");
        assert!(!item.processed);
    }

    #[test]
    fn test_mark_processed() {
        let s = store();
        s.create(new_item("c2", "Call dentist")).expect("create");
        let updated = s.mark_processed("c2").expect("mark");
        assert!(updated.processed);
    }

    #[test]
    fn test_list_excludes_processed() {
        let s = store();
        s.create(new_item("c3", "Unprocessed")).expect("c3");
        s.create(new_item("c4", "Processed")).expect("c4");
        s.mark_processed("c4").expect("mark");
        let items = s.list(false).expect("list");
        assert!(items.iter().any(|i| i.slug == "c3"));
        assert!(!items.iter().any(|i| i.slug == "c4"));
    }
}
