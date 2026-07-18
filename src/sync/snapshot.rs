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
//! optional fields with `#[serde(default)]`) do NOT require a bump, but a bump
//! is recommended when semantics change (e.g. checklist items live only under
//! `tasks` with `kind`, not dual-written to `todos`).

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{CaptureItem, Note, Project, Reminder, Task, TimeEntry, Todo};

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
    /// All task records (including `checklist_item` kind) at snapshot time.
    pub tasks: Vec<Task>,
    /// Legacy todo array for inbound compatibility with pre-1.1 remotes.
    ///
    /// On **outbound** snapshots this is always empty — checklist items are
    /// carried under [`Self::tasks`] with `kind = checklist_item`. On
    /// **inbound**, non-empty `todos` are still applied so older peers can
    /// push checklist data.
    #[serde(default)]
    pub todos: Vec<Todo>,
    /// All time entry records at snapshot time.
    pub time_entries: Vec<TimeEntry>,
    /// All reminder records at snapshot time.
    pub reminders: Vec<Reminder>,
    /// All capture-inbox items at snapshot time.
    pub capture_items: Vec<CaptureItem>,
    /// All notes at snapshot time (PKM layer).
    #[serde(default)]
    pub notes: Vec<Note>,
}

// ── snapshot impl ──────────────────────────────────────────────────────────

impl StateSnapshot {
    /// Snapshot schema version — bump on every breaking format change.
    ///
    /// Breaking changes include removing or renaming fields, changing a field's
    /// type incompatibly, or reordering enum variants. Additive changes (adding
    /// new optional fields) do NOT require a bump. Remote providers use this
    /// value to reject snapshots they cannot interpret.
    ///
    /// **v2** — hierarchical tasks (`parent_slug` / `kind` on `Task`), notes in
    /// the snapshot, and checklist items carried only under `tasks` (outbound
    /// `todos` is empty).
    pub const SCHEMA_VERSION: u32 = 2;

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
            notes: &self.notes,
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
    /// Checklist items are included under [`Self::tasks`] with
    /// `kind = checklist_item`. [`Self::todos`] is left empty on outbound so
    /// entities are not dual-represented.
    ///
    /// # Errors
    ///
    /// Returns an error if any database query fails.
    pub fn from_db(conn: &Arc<Mutex<Connection>>, machine_id: Uuid) -> anyhow::Result<Self> {
        use crate::store::{
            SqliteCaptureItems, SqliteNotes, SqliteProjects, SqliteReminders, SqliteTasks,
            SqliteTimeEntries,
        };

        let projects = SqliteProjects::new(Arc::clone(conn)).list_all()?;
        let tasks = SqliteTasks::new(Arc::clone(conn)).list_all()?;
        let time_entries = SqliteTimeEntries::new(Arc::clone(conn)).list_all()?;
        let reminders = SqliteReminders::new(Arc::clone(conn)).list_all()?;
        let capture_items = SqliteCaptureItems::new(Arc::clone(conn)).list_all()?;
        let notes = SqliteNotes::new(Arc::clone(conn)).list_all()?;

        Ok(Self {
            snapshot_at: Utc::now(),
            machine_id,
            schema_version: Self::SCHEMA_VERSION,
            projects,
            tasks,
            // Outbound: no dual write of checklist_items as todos.
            todos: Vec::new(),
            time_entries,
            reminders,
            capture_items,
            notes,
        })
    }

    /// Writes all entities in this snapshot to the database using upsert semantics.
    ///
    /// For entities with foreign keys (tasks, todos, reminders, `time_entries`),
    /// this uses slug resolution to properly map remote project/task slugs to
    /// local numeric IDs, avoiding foreign key mismatches when syncing from
    /// upstream.
    ///
    /// After notes are written, `[[slug]]` backlinks are rebuilt from content.
    ///
    /// # Errors
    ///
    /// Returns an error if any database write fails.
    pub fn write_to_db(&self, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<()> {
        use crate::domain::{Links, parse_links};
        use crate::store::{
            SqliteCaptureItems, SqliteLinks, SqliteNotes, SqliteProjects, SqliteReminders,
            SqliteTasks, SqliteTimeEntries, SqliteTodos,
        };

        tracing::debug!(
            projects = self.projects.len(),
            tasks = self.tasks.len(),
            todos = self.todos.len(),
            time_entries = self.time_entries.len(),
            reminders = self.reminders.len(),
            capture_items = self.capture_items.len(),
            notes = self.notes.len(),
            "write_to_db: starting"
        );

        SqliteProjects::new(Arc::clone(conn)).upsert_all(&self.projects)?;
        SqliteTasks::new(Arc::clone(conn)).upsert_all_with_slug_resolution(&self.tasks)?;
        // Inbound legacy: apply remote todos as checklist_item rows.
        if !self.todos.is_empty() {
            SqliteTodos::new(Arc::clone(conn)).upsert_all_with_slug_resolution(&self.todos)?;
        }
        SqliteTimeEntries::new(Arc::clone(conn))
            .upsert_all_with_slug_resolution(&self.time_entries)?;
        SqliteReminders::new(Arc::clone(conn)).upsert_all_with_slug_resolution(&self.reminders)?;
        SqliteCaptureItems::new(Arc::clone(conn)).upsert_all(&self.capture_items)?;
        SqliteNotes::new(Arc::clone(conn)).upsert_all(&self.notes)?;

        // Rebuild backlinks from note content after notes are persisted.
        let links = SqliteLinks::new(Arc::clone(conn));
        for note in &self.notes {
            links.delete_all_for(&note.slug)?;
            for target in parse_links(&note.content) {
                if target != note.slug {
                    let _ = links.create(&note.slug, &target);
                }
            }
        }

        tracing::debug!("write_to_db: complete");
        Ok(())
    }

    /// Returns the total count of all entities across all tables.
    ///
    /// Counts `tasks` + legacy `todos` + other domains. Callers that build
    /// snapshots via [`Self::from_db`] will not double-count checklist items
    /// because outbound `todos` is empty.
    #[must_use]
    pub fn entities(&self) -> usize {
        self.projects.len()
            + self.tasks.len()
            + self.todos.len()
            + self.time_entries.len()
            + self.reminders.len()
            + self.capture_items.len()
            + self.notes.len()
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
    notes: &'a [Note],
}
