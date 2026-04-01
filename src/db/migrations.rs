// Rust guideline compliant 2026-02-21
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
    vec![M::up(M1), M::up(M2)]
}
