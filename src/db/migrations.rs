//! Embedded SQL migration definitions for Scribe's `SQLite` database.
//!
//! Each migration is a `&str` constant containing valid `SQLite` DDL/DML.
//! Migrations are applied in order by [`rusqlite_migration`] on every
//! application startup. Once applied, a migration is never re-run.
//!
//! # Migrations
//!
//! - **M1** — creates all six core tables and seeds the reserved
//!   `quick-capture` project.
//! - **M2** — adds the `persistent` column to the `reminders` table.
//! - **M3** — creates the `sync_metadata` table.
//! - **M4** — adds `parent_id` and `kind` columns to the `tasks` table.
//! - **M5** — migrates all `todos` rows into `tasks` (as `checklist_item` kind) and drops `todos`.
//! - **M6** — creates `notes` and `links` tables for PKM functionality.
//! - **M7** — creates FTS5 virtual table for full-text search on notes.

use rusqlite_migration::M;

/// Initial schema migration: all tables + `quick-capture` seed row.
///
/// Creates `projects`, `tasks`, `todos`, `time_entries`, `capture_items`,
/// and `reminders` tables with all indexes. Seeds the reserved
/// `quick-capture` inbox project using `INSERT OR IGNORE` so the statement
/// is idempotent.
pub(super) const M1: &str = "
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS projects (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    description TEXT,
    status      TEXT    NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active', 'paused', 'completed')),
    is_reserved INTEGER NOT NULL DEFAULT 0,
    archived_at TEXT,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    title       TEXT    NOT NULL,
    description TEXT,
    status      TEXT    NOT NULL DEFAULT 'todo'
                        CHECK (status IN ('todo', 'in_progress', 'done', 'cancelled')),
    priority    TEXT    NOT NULL DEFAULT 'medium'
                        CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
    due_date    TEXT,
    archived_at TEXT,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_project_id  ON tasks(project_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status       ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_due_date     ON tasks(due_date);

CREATE TABLE IF NOT EXISTS todos (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    title       TEXT    NOT NULL,
    done        INTEGER NOT NULL DEFAULT 0,
    archived_at TEXT,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_todos_project_id ON todos(project_id);

CREATE TABLE IF NOT EXISTS time_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id     INTEGER          REFERENCES tasks(id)    ON DELETE SET NULL,
    started_at  TEXT    NOT NULL,
    ended_at    TEXT,
    note        TEXT,
    archived_at TEXT,
    created_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_time_entries_project_id ON time_entries(project_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_task_id    ON time_entries(task_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_started_at ON time_entries(started_at);

CREATE TABLE IF NOT EXISTS capture_items (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,
    body        TEXT    NOT NULL,
    processed   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS reminders (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id     INTEGER          REFERENCES tasks(id)    ON DELETE SET NULL,
    remind_at   TEXT    NOT NULL,
    message     TEXT,
    fired       INTEGER NOT NULL DEFAULT 0,
    archived_at TEXT,
    created_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reminders_project_id ON reminders(project_id);
CREATE INDEX IF NOT EXISTS idx_reminders_task_id    ON reminders(task_id);
CREATE INDEX IF NOT EXISTS idx_reminders_remind_at  ON reminders(remind_at);

INSERT OR IGNORE INTO projects (slug, name, status, is_reserved, created_at, updated_at)
VALUES ('quick-capture', 'Quick Capture', 'active', 1,
        datetime('now'), datetime('now'));
";

// ── migrations ─────────────────────────────────────────────────────────────

// ── migrations ─────────────────────────────────────────────────────────────

/// M2 — adds the `persistent` column to `reminders`.
///
/// `persistent = 1` causes the notification to use a blocking `display alert`
/// on macOS (stays until the user clicks Dismiss) rather than a self-dismissing
/// banner. Existing rows default to `0` (non-persistent).
pub(super) const M2: &str =
    "ALTER TABLE reminders ADD COLUMN persistent INTEGER NOT NULL DEFAULT 0;";

/// M3 — creates the `sync_metadata` table for storing sync-related key-value data.
///
/// Stores the last successful [`SyncSummary`][crate::sync::SyncSummary] as JSON
/// alongside other sync-related metadata keys.
pub(super) const M3: &str = "
CREATE TABLE IF NOT EXISTS sync_metadata (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);";

/// M4 — adds `parent_id` and `kind` columns to the `tasks` table.
///
/// `parent_id` enables infinite hierarchical nesting (task → sub-task → sub-sub-task).
/// It is nullable — a `NULL` parent means a top-level task.
///
/// `kind` distinguishes between a full task (`task`) and a lightweight checklist
/// item (`checklist_item`). This allows UI rendering to treat deep sub-tasks as
/// simple checkbox items while keeping the same underlying storage.
pub(super) const M4: &str = "
ALTER TABLE tasks ADD COLUMN parent_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL;
ALTER TABLE tasks ADD COLUMN kind TEXT NOT NULL DEFAULT 'task'
    CHECK (kind IN ('task', 'checklist_item'));
CREATE INDEX IF NOT EXISTS idx_tasks_parent_id ON tasks(parent_id);";

/// M5 — migrates all `todos` rows into `tasks` as `checklist_item` kind and drops `todos`.
///
/// Each existing `todo` becomes a top-level task with:
/// - `slug` preserved; if a collision exists with an existing task, appends `-migrated`
///   with a timestamp suffix to ensure uniqueness.
/// - `status` mapped from `done` — `done = 1` → `status = 'done'`, `done = 0` → `status = 'todo'`.
/// - `kind = 'checklist_item'` — todos become checklist items, not full tasks.
/// - `parent_id = NULL` — todos become top-level (no parent task concept existed).
/// - `priority = 'medium'` and `description = NULL` — reasonable defaults for migrated items.
/// - `archived_at`, `created_at` preserved as-is.
///
/// After all rows are inserted, the `todos` table is dropped.
pub(super) const M5: &str = "
INSERT INTO tasks
    (slug, project_id, title, status, priority, description, parent_id, kind, archived_at, created_at, updated_at)
SELECT
    CASE
        WHEN (SELECT COUNT(*) FROM tasks t WHERE t.slug = todos.slug) > 0
        THEN todos.slug || '-migrated-' || unixepoch('now')
        ELSE todos.slug
    END,
    project_id,
    title,
    CASE WHEN done = 1 THEN 'done' ELSE 'todo' END,
    'medium',
    NULL,
    NULL,
    'checklist_item',
    archived_at,
    created_at,
    created_at
FROM todos;
DROP TABLE IF EXISTS todos;";

/// M6 — creates `notes` and `links` tables for PKM (Personal Knowledge Management).
///
/// `notes` stores markdown documents with a user-provided slug (no auto-prefix).
/// Slugs must be valid kebab-case identifiers.
///
/// `links` implements the bi-directional link layer: every note can reference
/// any other note or task slug, and the relationship is stored explicitly so
/// backlinks (e.g. "哪些笔记引用了这个笔记") can be queried efficiently.
pub(super) const M6: &str = "
CREATE TABLE IF NOT EXISTS notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,
    title       TEXT    NOT NULL,
    content     TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_notes_slug    ON notes(slug);
CREATE INDEX IF NOT EXISTS idx_notes_title  ON notes(title);

CREATE TABLE IF NOT EXISTS links (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    source_slug  TEXT    NOT NULL,
    target_slug  TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_slug, target_slug)
);

CREATE INDEX IF NOT EXISTS idx_links_source  ON links(source_slug);
CREATE INDEX IF NOT EXISTS idx_links_target  ON links(target_slug);";

/// M7 — creates FTS5 virtual table for full-text search on notes.
///
/// The `notes_fts` table indexes `title` and `content` columns from `notes`
/// using `SQLite`'s FTS5 module. The content is synchronized via triggers so that
/// inserts/updates/deletes on `notes` automatically update the FTS index.
///
/// FTS query syntax supports:
/// - `word` — simple term search
/// - `"phrase"` — exact phrase search
/// - `word*` — prefix matching
/// - `AND`, `OR` — boolean operators
pub(super) const M7: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
    title,
    content,
    content='notes',
    content_rowid='id'
);

CREATE TRIGGER IF NOT EXISTS notes_fts_insert AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, title, content) VALUES (new.id, new.title, new.content);
END;

CREATE TRIGGER IF NOT EXISTS notes_fts_update AFTER UPDATE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES('delete', old.id, old.title, old.content);
    INSERT INTO notes_fts(rowid, title, content) VALUES (new.id, new.title, new.content);
END;

CREATE TRIGGER IF NOT EXISTS notes_fts_delete AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES('delete', old.id, old.title, old.content);
END;";

/// Returns all migrations in application order.
///
/// Pass the returned slice to [`rusqlite_migration::Migrations::new`].
///
/// # Examples
///
/// ```ignore
/// let migrations = rusqlite_migration::Migrations::new(scribe::db::migrations::all());
/// ```
pub(super) fn all() -> Vec<M<'static>> {
    vec![
        M::up(M1),
        M::up(M2),
        M::up(M3),
        M::up(M4),
        M::up(M5),
        M::up(M6),
        M::up(M7),
    ]
}
