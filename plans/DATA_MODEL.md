# Scribe — Data Model

**Guideline compliance date: 2026-02-21**

---

## Design Decisions

### Slugs as the universal user-facing identifier

Every entity exposes a **globally unique slug** — numeric primary keys are
internal only and never surfaced to the user in the CLI or TUI.

| Entity | Slug format | Example |
|--------|-------------|---------|
| Project | user-defined kebab-case | `payments` |
| Task | `{project}-task-{title-slug}[-{4-char suffix}]` | `payments-task-fix-login` |
| Todo | `{project}-todo-{title-slug}[-{suffix}]` | `payments-todo-review-docs` |
| Reminder | `{project}-reminder-{title-slug}[-{suffix}]` | `payments-reminder-deploy-friday` |
| TimeEntry | `{project}-entry-{YYYYMMDD}-{HHmmss}[-{suffix}]` | `payments-entry-20260331-143000` |
| CaptureItem | `capture-{YYYYMMDD}-{HHmmss}[-{suffix}]` | `capture-20260331-143000` |

Auto-generated slugs are derived by lowercasing the title, stripping
non-alphanumeric characters, collapsing whitespace/punctuation to `-`, and
truncating. A 4-character random alphanumeric suffix is appended only on
collision.

Slug generation lives in `src/domain/slug.rs` and is tested exhaustively.

### Everything belongs to a project

`project_id` is `NOT NULL` on every entity except `CaptureItem`.
This eliminates nullable FK ambiguity and makes filtering, reporting, and
archiving uniform across the entire data set.

### The `quick-capture` reserved project

Seeded on first run with `id = 1`, `slug = "quick-capture"`, `is_reserved = 1`.
It acts as the default inbox for all items created without an explicit project.
It cannot be deleted or archived. Items inside it are recategorised by updating
`project_id` to point at a real project.

### Slugs as human-facing identifiers

Projects expose a `slug` (unique, kebab-case, e.g. `payment-automation`) which
is used everywhere in the CLI and TUI instead of raw numeric IDs. Internally,
foreign keys always reference the numeric `id` for performance and referential
integrity. The ops layer resolves slugs to IDs transparently.

### Soft delete via `archived_at`

No user data is hard-deleted. Every mutable entity carries an `archived_at`
column (`TEXT`, ISO 8601, nullable). `NULL` means active; a timestamp means
archived. Default queries always add `WHERE archived_at IS NULL`. An Archives
view shows archived items and allows restoration.

When a project is archived, all its linked tasks, todos, time entries, and
reminders are also archived in a single transaction. Restoring a project does
**not** automatically restore its items — the user restores items individually
from the Archives view.

### `ON DELETE RESTRICT` on project FKs

Because we never hard-delete projects containing items, `RESTRICT` is the
correct constraint. The ops layer enforces the archive-before-delete flow; the
DB constraint is a safety net.

### `ON DELETE SET NULL` on task FKs

`time_entries.task_id` and `reminders.task_id` are optional. If a task is
archived (not deleted — see above), these FKs are left as-is. The `SET NULL`
behaviour only applies if a task row is ever physically removed (which should
not happen under normal operation).

---

## Full Schema (SQLite)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- ── projects ──────────────────────────────────────────────────────────────
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

-- Seed the reserved quick-capture project (migration 0001).
-- INSERT OR IGNORE INTO projects (slug, name, status, is_reserved, created_at, updated_at)
-- VALUES ('quick-capture', 'Quick Capture', 'active', 1,
--         datetime('now'), datetime('now'));

-- ── tasks ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS tasks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,       -- e.g. payments-task-fix-login
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    title       TEXT    NOT NULL,
    description TEXT,
    status      TEXT    NOT NULL DEFAULT 'todo'
                        CHECK (status IN ('todo', 'in_progress', 'done', 'cancelled')),
    priority    TEXT    NOT NULL DEFAULT 'medium'
                        CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
    due_date    TEXT,                        -- ISO 8601 date string; nullable
    archived_at TEXT,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_project_id  ON tasks(project_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status       ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_due_date     ON tasks(due_date);

-- ── todos ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS todos (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,       -- e.g. payments-todo-review-docs
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    title       TEXT    NOT NULL,
    done        INTEGER NOT NULL DEFAULT 0,
    archived_at TEXT,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_todos_project_id ON todos(project_id);

-- ── time_entries ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS time_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,       -- e.g. payments-entry-20260331-143000
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id     INTEGER          REFERENCES tasks(id)    ON DELETE SET NULL,
    started_at  TEXT    NOT NULL,            -- ISO 8601 datetime
    ended_at    TEXT,                        -- NULL = timer still running
    note        TEXT,
    archived_at TEXT,
    created_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_time_entries_project_id ON time_entries(project_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_task_id    ON time_entries(task_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_started_at ON time_entries(started_at);

-- ── capture_items ─────────────────────────────────────────────────────────
-- Raw inbox — intentionally has no project binding.
-- Items are triaged via `scribe inbox process <slug>`.
CREATE TABLE IF NOT EXISTS capture_items (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,       -- e.g. capture-20260331-143000
    body        TEXT    NOT NULL,
    processed   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL
);

-- ── reminders ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS reminders (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT    NOT NULL UNIQUE,       -- e.g. payments-reminder-deploy-friday
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id     INTEGER          REFERENCES tasks(id)    ON DELETE SET NULL,
    remind_at   TEXT    NOT NULL,            -- ISO 8601 datetime
    message     TEXT,
    fired       INTEGER NOT NULL DEFAULT 0,
    archived_at TEXT,
    created_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reminders_project_id ON reminders(project_id);
CREATE INDEX IF NOT EXISTS idx_reminders_task_id    ON reminders(task_id);
CREATE INDEX IF NOT EXISTS idx_reminders_remind_at  ON reminders(remind_at);
```

---

## Entity Relationship Summary

```
projects (1) ──< tasks          (project_id NOT NULL)
projects (1) ──< todos          (project_id NOT NULL)
projects (1) ──< time_entries   (project_id NOT NULL)
projects (1) ──< reminders      (project_id NOT NULL)

tasks (0..1) ──< time_entries   (task_id nullable)
tasks (0..1) ──< reminders      (task_id nullable)

capture_items                   (no FK — raw inbox)
```

---

## Rust Domain Types

### Newtype IDs

```rust
pub struct ProjectId(pub i64);
pub struct TaskId(pub i64);
pub struct TodoId(pub i64);
pub struct TimeEntryId(pub i64);
pub struct CaptureItemId(pub i64);
pub struct ReminderId(pub i64);
```

All ID newtypes derive `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`.

### Status and Priority Enums

```rust
pub enum ProjectStatus { Active, Paused, Completed }

pub enum TaskStatus { Todo, InProgress, Done, Cancelled }

pub enum TaskPriority { Low, Medium, High, Urgent }
```

### Structs

Each struct mirrors the DB schema 1-to-1. `archived_at: Option<DateTime<Utc>>`.
Numeric IDs are internal only; the `slug` field is the user-facing identifier.

```rust
pub struct Project {
    pub id:          ProjectId,
    pub slug:        String,
    pub name:        String,
    pub description: Option<String>,
    pub status:      ProjectStatus,
    pub is_reserved: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

pub struct Task {
    pub id:          TaskId,
    pub slug:        String,
    pub project_id:  ProjectId,
    pub title:       String,
    pub description: Option<String>,
    pub status:      TaskStatus,
    pub priority:    TaskPriority,
    pub due_date:    Option<NaiveDate>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

pub struct Todo {
    pub id:          TodoId,
    pub slug:        String,
    pub project_id:  ProjectId,
    pub title:       String,
    pub done:        bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

pub struct TimeEntry {
    pub id:          TimeEntryId,
    pub slug:        String,
    pub project_id:  ProjectId,
    pub task_id:     Option<TaskId>,
    pub started_at:  DateTime<Utc>,
    pub ended_at:    Option<DateTime<Utc>>,
    pub note:        Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at:  DateTime<Utc>,
}

pub struct CaptureItem {
    pub id:         CaptureItemId,
    pub slug:       String,
    pub body:       String,
    pub processed:  bool,
    pub created_at: DateTime<Utc>,
}

pub struct Reminder {
    pub id:          ReminderId,
    pub slug:        String,
    pub project_id:  ProjectId,
    pub task_id:     Option<TaskId>,
    pub remind_at:   DateTime<Utc>,
    pub message:     Option<String>,
    pub fired:       bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at:  DateTime<Utc>,
}
```

---

## Migration Strategy

Migrations live in `src/db/migrations.rs` as embedded SQL strings using
`rusqlite_migration`. Each migration is numbered sequentially (M1, M2, …).
Migration M1 creates all tables and seeds the `quick-capture` project.

The migration runner executes on every startup before any query, making schema
upgrades transparent to the user.

---

## Slug Generation

Slug generation logic lives in `src/domain/slug.rs`. The algorithm:

1. Lowercase the input string.
2. Replace any run of non-alphanumeric characters with a single `-`.
3. Strip leading and trailing `-`.
4. Truncate to 40 characters at a word boundary where possible.
5. Prepend entity prefix (e.g. `{project}-task-`).
6. Check uniqueness in the DB; if collision, append a 4-char random alphanumeric
   suffix and retry (max 5 attempts before returning an error).

The module is unit-tested exhaustively including collision handling.

---

## Tab Completion

A hidden subcommand `scribe __complete <entity>` returns completion candidates
for the shell completion scripts. Output format is one `<slug>\t<hint>` pair
per line, e.g.:

```
payments-task-fix-login    Fix the login bug on prod
payments-task-write-tests  Write unit tests for auth module
```

Supported entities: `projects`, `tasks`, `todos`, `reminders`, `entries`,
`captures`. The subcommand always reads from the active (non-archived) set
unless `--archived` is passed.
