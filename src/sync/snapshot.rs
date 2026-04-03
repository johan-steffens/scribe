// Rust guideline compliant 2026-02-21
//! `StateSnapshot` — a flat, serialisable point-in-time view of all entities.
//!
//! A snapshot captures every entity table in the database into a single
//! JSON-serialisable document. It is used by the sync engine to determine
//! whether a push is needed and to transfer state to a remote provider.
//!
//! # Content hash
//!
//! [`StateSnapshot::content_hash`] returns a hex-encoded SHA-256 digest of the
//! snapshot's *data* fields. Metadata fields (`snapshot_at`, `machine_id`) are
//! deliberately excluded so that two snapshots taken at different times on
//! different machines but with identical data produce the same hash. This makes
//! the hash safe to use as an idempotency key for push operations.
//!
//! # Schema versioning
//!
//! [`StateSnapshot::SCHEMA_VERSION`] must be bumped whenever a breaking change
//! is made to the snapshot format (e.g. a field is removed, renamed, or its
//! type changes in a non-backwards-compatible way). Additive changes (new
//! optional fields) do NOT require a bump.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{CaptureItem, Project, Reminder, Task, TimeEntry, Todo};

// ── snapshot struct ────────────────────────────────────────────────────────

/// A flat, serialisable point-in-time view of all database entities.
///
/// Captures every entity table in a single document for transfer to, or
/// comparison with, a remote sync provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// UTC timestamp when this snapshot was taken.
    pub snapshot_at: DateTime<Utc>,
    /// UUID identifying the machine that produced this snapshot.
    pub machine_id: Uuid,
    /// Schema version; see [`StateSnapshot::SCHEMA_VERSION`].
    pub schema_version: u32,
    /// All project records at snapshot time.
    pub projects: Vec<Project>,
    /// All task records at snapshot time.
    pub tasks: Vec<Task>,
    /// All todo records at snapshot time.
    pub todos: Vec<Todo>,
    /// All time entry records at snapshot time.
    pub time_entries: Vec<TimeEntry>,
    /// All reminder records at snapshot time.
    pub reminders: Vec<Reminder>,
    /// All capture-inbox items at snapshot time.
    pub capture_items: Vec<CaptureItem>,
}

// ── snapshot impl ──────────────────────────────────────────────────────────

impl StateSnapshot {
    /// Snapshot schema version — bump on every breaking format change.
    ///
    /// Breaking changes include removing or renaming fields, changing a field's
    /// type incompatibly, or reordering enum variants. Additive changes (adding
    /// new optional fields) do NOT require a bump. Remote providers use this
    /// value to reject snapshots they cannot interpret.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Returns a hex-encoded SHA-256 hash of the snapshot's data content.
    ///
    /// The hash covers all entity data and `schema_version`, but **excludes**
    /// `snapshot_at` and `machine_id`. Two snapshots taken at different times
    /// on different machines but with identical data will produce the same
    /// hash, making it safe to use as a push-idempotency key.
    ///
    /// # Panics
    ///
    /// Panics if the hashable fields cannot be serialised to JSON. This should
    /// never occur for well-formed domain types and would indicate a
    /// programming error (e.g. a non-serialisable custom type was introduced).
    #[must_use]
    pub fn content_hash(&self) -> String {
        let hashable = HashableSnapshot {
            schema_version: self.schema_version,
            projects: &self.projects,
            tasks: &self.tasks,
            todos: &self.todos,
            time_entries: &self.time_entries,
            reminders: &self.reminders,
            capture_items: &self.capture_items,
        };

        // Serialise to JSON bytes, then SHA-256 hash, then hex-encode.
        // Panicking here is correct: a serialisation failure means a
        // programming error (M-PANIC-ON-BUG).
        let json =
            serde_json::to_vec(&hashable).expect("invariant: domain types must be serialisable");

        let digest = Sha256::digest(&json);
        hex::encode(digest)
    }

    /// Builds a snapshot from the live database including all rows (even archived).
    ///
    /// # Errors
    ///
    /// Returns an error if any database query fails.
    pub fn from_db(conn: &Arc<Mutex<Connection>>, machine_id: Uuid) -> anyhow::Result<Self> {
        use crate::store::{
            SqliteCaptureItems, SqliteProjects, SqliteReminders, SqliteTasks, SqliteTimeEntries,
            SqliteTodos,
        };

        let projects = SqliteProjects::new(Arc::clone(conn)).list_all()?;
        let tasks = SqliteTasks::new(Arc::clone(conn)).list_all()?;
        let todos = SqliteTodos::new(Arc::clone(conn)).list_all()?;
        let time_entries = SqliteTimeEntries::new(Arc::clone(conn)).list_all()?;
        let reminders = SqliteReminders::new(Arc::clone(conn)).list_all()?;
        let capture_items = SqliteCaptureItems::new(Arc::clone(conn)).list_all()?;

        Ok(Self {
            snapshot_at: Utc::now(),
            machine_id,
            schema_version: Self::SCHEMA_VERSION,
            projects,
            tasks,
            todos,
            time_entries,
            reminders,
            capture_items,
        })
    }

    /// Writes all entities in this snapshot to the database using upsert semantics.
    ///
    /// # Errors
    ///
    /// Returns an error if any database write fails.
    pub fn write_to_db(&self, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<()> {
        use crate::store::{
            SqliteCaptureItems, SqliteProjects, SqliteReminders, SqliteTasks, SqliteTimeEntries,
            SqliteTodos,
        };

        SqliteProjects::new(Arc::clone(conn)).upsert_all(&self.projects)?;
        SqliteTasks::new(Arc::clone(conn)).upsert_all(&self.tasks)?;
        SqliteTodos::new(Arc::clone(conn)).upsert_all(&self.todos)?;
        SqliteTimeEntries::new(Arc::clone(conn)).upsert_all(&self.time_entries)?;
        SqliteReminders::new(Arc::clone(conn)).upsert_all(&self.reminders)?;
        SqliteCaptureItems::new(Arc::clone(conn)).upsert_all(&self.capture_items)?;
        Ok(())
    }

    /// Returns the total count of all entities across all tables.
    #[must_use]
    pub fn entities(&self) -> usize {
        self.projects.len()
            + self.tasks.len()
            + self.todos.len()
            + self.time_entries.len()
            + self.reminders.len()
            + self.capture_items.len()
    }
}

// ── internal hashable projection ───────────────────────────────────────────

/// Internal projection used for content hashing; excludes metadata fields.
///
/// `snapshot_at` and `machine_id` are omitted so that the hash reflects only
/// the *data* content of the snapshot, not when or where it was created.
#[derive(Serialize)]
struct HashableSnapshot<'a> {
    schema_version: u32,
    projects: &'a [Project],
    tasks: &'a [Task],
    todos: &'a [Todo],
    time_entries: &'a [TimeEntry],
    reminders: &'a [Reminder],
    capture_items: &'a [CaptureItem],
}
