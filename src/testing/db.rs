// Temporary database creation helpers for tests.
//
// Provides [`TestDb`] for creating isolated in-memory or temporary file databases
// with automatic migrations applied. Each instance is independent and cleaned up
// when dropped.

use std::sync::{Arc, Mutex};

/// A temporary test database that is automatically cleaned up on drop.
///
/// Wraps an in-memory SQLite database with all migrations applied. The `quick-capture`
/// seed project is guaranteed to exist after creation.
///
/// # Differences from `TestDb::tempfile()`
///
/// | Method       | Storage | Shared across threads | Use case                          |
/// |--------------|---------|----------------------|------------------------------------|
/// | `new()`      | memory  | yes                  | Fast unit tests, single-threaded  |
/// | `tempfile()` | file    | yes                  | Debugging (inspect the `.db` file) |
///
/// # Example
///
/// ```
/// let test_db = crate::testing::db::TestDb::new();
/// let conn = test_db.conn();
///
/// // Use conn with the store/ops layers...
/// ```
#[derive(Debug)]
pub struct TestDb {
    /// Guards the connection behind a mutex for store/ops compatibility.
    conn: Arc<Mutex<rusqlite::Connection>>,
    /// Keeps the temp directory alive for the lifetime of the database.
    #[allow(dead_code)]
    dir: tempfile::TempDir,
}

impl TestDb {
    /// Creates a new in-memory database with all migrations applied.
    ///
    /// This is the fastest option and is suitable for most unit tests.
    /// The database is freed when this instance is dropped.
    pub fn new() -> Self {
        let conn = Arc::new(Mutex::new(
            crate::db::open_in_memory().expect("in-memory DB should succeed"),
        ));
        Self {
            conn,
            dir: tempfile::TempDir::new_in("/tmp").expect("tempdir should succeed"),
        }
    }

    /// Creates a new temporary file database with all migrations applied.
    ///
    /// The file is placed in the OS temp directory and deleted when the
    /// instance is dropped. Use this variant when you need to inspect
    /// the database file directly (e.g., during debugging).
    pub fn tempfile() -> Self {
        let dir = tempfile::tempdir().expect("tempdir should succeed");
        let db_path = dir.path().join("test.db");
        let conn = Arc::new(Mutex::new(
            crate::db::open(&db_path).expect("tempfile DB should succeed"),
        ));
        // Keep `dir` alive for the lifetime of `self` — dropping it would delete
        // the directory and invalidate any active file handles held by the Connection.
        Self { conn, dir }
    }

    /// Creates a new temporary file database in the provided temp directory.
    ///
    /// The caller is responsible for ensuring `dir` outlives `self`.
    /// This is useful when you need to know the database path (e.g., for pairing
    /// with [`TestConfig::with_db_path`][crate::testing::config::TestConfig::with_db_path]).
    pub fn new_in_dir(dir: tempfile::TempDir) -> Self {
        let db_path = dir.path().join("test.db");
        let conn = Arc::new(Mutex::new(
            crate::db::open(&db_path).expect("tempfile DB should succeed"),
        ));
        Self { conn, dir }
    }

    /// Returns the path to the database file.
    pub fn db_path(&self) -> std::path::PathBuf {
        self.dir.path().join("test.db")
    }

    /// Returns a shared, mutex-guarded connection to the underlying database.
    ///
    /// The connection type matches what the store and ops layers expect:
    /// `Arc<Mutex<rusqlite::Connection>>`.
    #[must_use]
    pub fn conn(&self) -> Arc<Mutex<rusqlite::Connection>> {
        Arc::clone(&self.conn)
    }

    /// Returns a reference to the underlying mutex-guarded connection.
    ///
    /// Prefer [`Self::conn()`] when you need to pass ownership of the guard.
    #[must_use]
    pub fn conn_ref(&self) -> &Arc<Mutex<rusqlite::Connection>> {
        &self.conn
    }
}

impl Default for TestDb {
    fn default() -> Self {
        Self::new()
    }
}
