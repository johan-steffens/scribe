//! Unit tests for [`crate::db`].

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
