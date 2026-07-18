//! Unit tests for [`crate::db`].

use rusqlite::Connection;

use scribe::db;
use scribe::testing::db::TestDb;

#[test]
fn test_open_in_memory_succeeds() {
    let test_db = TestDb::new();
    let conn = test_db.conn();
    // quick-capture project must be seeded
    let count: i64 = conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM projects WHERE slug = 'quick-capture'",
            [],
            |row| row.get(0),
        )
        .expect("query failed");
    assert_eq!(count, 1);
}

#[test]
fn test_open_creates_file_and_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("nested").join("scribe.db");
    let conn = db::open(&db_path).expect("should open");
    assert!(db_path.exists());
    drop(conn);
}

// ── M5: migrate todos to tasks ─────────────────────────────────────────────

/// Builds a pre-M5 schema: projects + tasks (with M4's `parent_id`/`kind`) + todos.
/// This mirrors the state of the database just before M5 runs.
fn conn_with_pre_m5_schema() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory conn");
    conn.execute("PRAGMA foreign_keys = ON", [])
        .expect("fk pragma");

    conn.execute_batch(
        "
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
            parent_id   INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
            kind        TEXT    NOT NULL DEFAULT 'task'
                                CHECK (kind IN ('task', 'checklist_item')),
            archived_at TEXT,
            created_at  TEXT    NOT NULL,
            updated_at  TEXT    NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tasks_project_id  ON tasks(project_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_status       ON tasks(status);
        CREATE INDEX IF NOT EXISTS idx_tasks_due_date     ON tasks(due_date);
        CREATE INDEX IF NOT EXISTS idx_tasks_parent_id    ON tasks(parent_id);

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

        -- seed quick-capture (id=1)
        INSERT OR IGNORE INTO projects (slug, name, status, is_reserved, created_at, updated_at)
        VALUES ('quick-capture', 'Quick Capture', 'active', 1,
                datetime('now'), datetime('now'));
        ",
    )
    .expect("schema setup");
    conn
}

/// The M5 migration SQL — identical to [`crate::db::migrations::M5`].
const M5_SQL: &str = "
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

fn apply_m5(conn: &Connection) {
    conn.execute_batch(M5_SQL).expect("M5 apply");
}

#[test]
fn test_m5_migrates_todos_to_tasks() {
    let conn = conn_with_pre_m5_schema();

    // Insert some todos (using project_id=1 which is quick-capture)
    conn.execute(
        "INSERT INTO todos (slug, project_id, title, done, created_at, updated_at)
         VALUES ('my-todo', 1, 'My Todo', 0, datetime('now'), datetime('now'))",
        [],
    )
    .expect("insert pending todo");
    conn.execute(
        "INSERT INTO todos (slug, project_id, title, done, created_at, updated_at)
         VALUES ('done-todo', 1, 'Done Todo', 1, datetime('now'), datetime('now'))",
        [],
    )
    .expect("insert done todo");

    apply_m5(&conn);

    // todos table must be gone
    let gone: Result<i64, _> = conn.query_row("SELECT COUNT(*) FROM todos", [], |row| row.get(0));
    assert!(gone.is_err(), "todos table should be dropped");

    // Check migrated tasks exist
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE slug IN ('my-todo', 'done-todo')",
            [],
            |row| row.get(0),
        )
        .expect("count query");
    assert_eq!(count, 2);

    // Verify pending todo became 'todo' status
    let pending_status: String = conn
        .query_row(
            "SELECT status FROM tasks WHERE slug = 'my-todo'",
            [],
            |row| row.get(0),
        )
        .expect("pending status");
    assert_eq!(pending_status, "todo");

    // Verify done todo became 'done' status
    let done_status: String = conn
        .query_row(
            "SELECT status FROM tasks WHERE slug = 'done-todo'",
            [],
            |row| row.get(0),
        )
        .expect("done status");
    assert_eq!(done_status, "done");

    // Verify kind is checklist_item
    let kind: String = conn
        .query_row("SELECT kind FROM tasks WHERE slug = 'my-todo'", [], |row| {
            row.get(0)
        })
        .expect("kind");
    assert_eq!(kind, "checklist_item");

    // Verify priority defaults to medium
    let priority: String = conn
        .query_row(
            "SELECT priority FROM tasks WHERE slug = 'my-todo'",
            [],
            |row| row.get(0),
        )
        .expect("priority");
    assert_eq!(priority, "medium");

    // Verify parent_id is NULL
    let parent_id: Option<i64> = conn
        .query_row(
            "SELECT parent_id FROM tasks WHERE slug = 'my-todo'",
            [],
            |row| row.get(0),
        )
        .expect("parent_id");
    assert_eq!(parent_id, None);
}

#[test]
fn test_m5_handles_slug_collision() {
    let conn = conn_with_pre_m5_schema();

    // Pre-create a task with the same slug as the todo we're about to insert
    conn.execute(
        "INSERT INTO tasks (slug, project_id, title, status, priority, kind, created_at, updated_at)
         VALUES ('collision-slug', 1, 'Existing Task', 'todo', 'medium', 'task',
                 datetime('now'), datetime('now'))",
        [],
    )
    .expect("insert existing task");

    // Insert a todo with the colliding slug
    conn.execute(
        "INSERT INTO todos (slug, project_id, title, done, created_at, updated_at)
         VALUES ('collision-slug', 1, 'Todo That Collides', 0, datetime('now'), datetime('now'))",
        [],
    )
    .expect("insert colliding todo");

    apply_m5(&conn);

    // The original task must still exist with its original slug and kind
    let original_still_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE slug = 'collision-slug' AND kind = 'task'",
            [],
            |row| row.get(0),
        )
        .expect("original task query");
    assert_eq!(original_still_exists, 1);

    // The migrated todo should have received a renamed slug
    let migrated_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE slug LIKE 'collision-slug-migrated-%'",
            [],
            |row| row.get(0),
        )
        .expect("migrated task query");
    assert_eq!(migrated_count, 1);
}

#[test]
fn test_m5_preserves_archived_at() {
    let conn = conn_with_pre_m5_schema();

    conn.execute(
        "INSERT INTO todos (slug, project_id, title, done, archived_at, created_at, updated_at)
         VALUES ('archived-todo', 1, 'Archived Todo', 0, datetime('now'), datetime('now'), datetime('now'))",
        [],
    )
    .expect("insert archived todo");

    apply_m5(&conn);

    let archived_at: Option<String> = conn
        .query_row(
            "SELECT archived_at FROM tasks WHERE slug = 'archived-todo'",
            [],
            |row| row.get(0),
        )
        .expect("archived_at");
    assert!(
        archived_at.is_some(),
        "archived_at must be preserved, got {archived_at:?}"
    );
}

#[test]
fn test_m5_preserves_created_at() {
    let conn = conn_with_pre_m5_schema();

    let created_at = "2024-01-01T00:00:00Z";
    conn.execute(
        "INSERT INTO todos (slug, project_id, title, done, archived_at, created_at, updated_at)
         VALUES ('dated-todo', 1, 'Dated Todo', 0, NULL, $created, $created)",
        [created_at],
    )
    .expect("insert dated todo");

    apply_m5(&conn);

    let task_created_at: String = conn
        .query_row(
            "SELECT created_at FROM tasks WHERE slug = 'dated-todo'",
            [],
            |row| row.get(0),
        )
        .expect("created_at");
    assert_eq!(task_created_at, created_at);
}

// ── M8: one running timer ──────────────────────────────────────────────────

/// The M8 migration SQL — identical to [`scribe::db::migrations`] M8.
const M8_SQL: &str = "
UPDATE time_entries
SET ended_at = started_at
WHERE ended_at IS NULL
  AND id NOT IN (
    SELECT id FROM (
      SELECT id FROM time_entries
      WHERE ended_at IS NULL
      ORDER BY started_at DESC, id DESC
      LIMIT 1
    )
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_time_entries_one_running
ON time_entries((1))
WHERE ended_at IS NULL;
";

#[test]
fn test_m8_rejects_second_running_timer() {
    let conn = db::open_in_memory().expect("in-memory");
    let now = "2026-01-01T12:00:00Z";
    conn.execute(
        "INSERT INTO time_entries (slug, project_id, started_at, created_at)
         VALUES ('runner-a', 1, ?1, ?1)",
        [now],
    )
    .expect("first runner");

    let err = conn
        .execute(
            "INSERT INTO time_entries (slug, project_id, started_at, created_at)
             VALUES ('runner-b', 1, ?1, ?1)",
            [now],
        )
        .expect_err("second running timer must violate unique index");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected unique constraint error, got: {err}"
    );
}

#[test]
fn test_m8_allows_multiple_completed_entries() {
    let conn = db::open_in_memory().expect("in-memory");
    for (slug, start, end) in [
        ("done-a", "2026-01-01T10:00:00Z", "2026-01-01T11:00:00Z"),
        ("done-b", "2026-01-01T12:00:00Z", "2026-01-01T13:00:00Z"),
    ] {
        conn.execute(
            "INSERT INTO time_entries (slug, project_id, started_at, ended_at, created_at)
             VALUES (?1, 1, ?2, ?3, ?2)",
            [slug, start, end],
        )
        .expect("insert completed");
    }
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM time_entries", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 2);
}

#[test]
fn test_m8_heals_duplicate_running_timers() {
    let conn = db::open_in_memory().expect("in-memory");
    // Drop the index so we can seed a dual-running state, then re-apply M8.
    conn.execute_batch("DROP INDEX IF EXISTS idx_time_entries_one_running;")
        .expect("drop index");
    conn.execute(
        "INSERT INTO time_entries (slug, project_id, started_at, created_at)
         VALUES ('old-runner', 1, '2026-01-01T10:00:00Z', '2026-01-01T10:00:00Z')",
        [],
    )
    .expect("old runner");
    conn.execute(
        "INSERT INTO time_entries (slug, project_id, started_at, created_at)
         VALUES ('new-runner', 1, '2026-01-01T12:00:00Z', '2026-01-01T12:00:00Z')",
        [],
    )
    .expect("new runner");

    conn.execute_batch(M8_SQL).expect("re-apply M8");

    let running: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM time_entries WHERE ended_at IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("running count");
    assert_eq!(running, 1);

    let kept: String = conn
        .query_row(
            "SELECT slug FROM time_entries WHERE ended_at IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("kept slug");
    assert_eq!(kept, "new-runner");
}
