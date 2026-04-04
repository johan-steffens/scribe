//! Database connection management and migration runner.
//!
//! This module is the single entry point for obtaining a `rusqlite::Connection`.
//! Every connection is configured with:
//!
//! - **WAL journal mode** — better concurrent read performance.
//! - **Foreign key enforcement** — `PRAGMA foreign_keys = ON`.
//! - **Automatic migrations** — [`rusqlite_migration`] runs pending migrations
//!   before the connection is returned to the caller.
//!
//! # Example
//!
//! ```no_run
//! use scribe::db::open;
//! use std::path::Path;
//!
//! let conn = open(Path::new("/tmp/scribe.db")).expect("failed to open DB");
//! ```

pub mod migrations;

use std::path::Path;

use rusqlite::Connection;
use rusqlite_migration::Migrations;

#[cfg(feature = "sync")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "sync")]
use crate::sync::SyncSummary;

/// Opens (or creates) the `SQLite` database at `path` and runs all pending migrations.
///
/// The returned connection has WAL mode and foreign-key constraints enabled.
/// Migrations are applied atomically; if a migration fails the connection is
/// not returned to the caller.
///
/// # Errors
///
/// Returns an error if the file cannot be opened, a PRAGMA fails, or any
/// migration cannot be applied.
///
/// # Examples
///
/// ```no_run
/// use scribe::db::open;
/// use std::path::Path;
///
/// let conn = open(Path::new("/tmp/scribe.db")).expect("DB open failed");
/// ```
pub fn open(path: &Path) -> anyhow::Result<Connection> {
    // Ensure the parent directory exists so rusqlite can create the file.
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }

    let mut conn = Connection::open(path)?;

    // Enable WAL mode for better concurrent read performance.
    // WAL is sticky — once set it persists across connections.
    conn.pragma_update(None, "journal_mode", "WAL")?;

    // Enforce referential integrity at the SQLite level.
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let migrations = Migrations::new(migrations::all());
    migrations.to_latest(&mut conn)?;

    tracing::debug!(
        db.path = %path.display(),
        "database opened and migrations applied",
    );

    Ok(conn)
}

/// Opens an in-memory `SQLite` database and runs all pending migrations.
///
/// Intended for unit tests. Each call returns an
/// independent, isolated DB instance — data is never persisted to disk.
///
/// # Errors
///
/// Returns an error if a migration cannot be applied.
///
/// # Examples
///
/// ```
/// let conn = scribe::db::open_in_memory().expect("in-memory DB failed");
/// ```
// Used in unit tests and #[cfg(test)] blocks throughout the crate.
#[allow(dead_code, reason = "used in test modules throughout the crate")]
pub fn open_in_memory() -> anyhow::Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let migrations = Migrations::new(migrations::all());
    migrations.to_latest(&mut conn)?;
    Ok(conn)
}

#[cfg(feature = "sync")]
const SYNC_SUMMARY_KEY: &str = "sync_summary";

#[cfg(feature = "sync")]
pub fn load_sync_summary(conn: &Arc<Mutex<Connection>>) -> Option<SyncSummary> {
    let conn = conn.lock().ok()?;
    let result: rusqlite::Result<String> = conn.query_row(
        "SELECT value FROM sync_metadata WHERE key = ?",
        [SYNC_SUMMARY_KEY],
        |row| row.get(0),
    );
    result
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}

/// Stores the sync summary in the database.
///
/// # Errors
///
/// Returns an error if the database connection fails or the JSON serialization fails.
#[cfg(feature = "sync")]
pub fn save_sync_summary(
    conn: &Arc<Mutex<Connection>>,
    summary: &SyncSummary,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(summary)?;
    let conn = conn
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    conn.execute(
        "INSERT OR REPLACE INTO sync_metadata (key, value, updated_at) VALUES (?, ?, datetime('now'))",
        [SYNC_SUMMARY_KEY, &json],
    )?;
    Ok(())
}

// ── test helpers ─────────────────────────────────────────────────────────

pub mod testing {
    //! Test helpers for the db module.
    //!
    //! Re-exports [`super::open_in_memory`] for convenience.

    pub use super::open_in_memory;
}
