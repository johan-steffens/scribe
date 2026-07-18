//! Unit tests for [`crate::store::time_entry_store::SqliteTimeEntries`].

use chrono::Utc;
use scribe::domain::{NewTimeEntry, ProjectId, TimeEntries};
use scribe::testing::time_entry_store;

use time_entry_store::store as make_store;

fn new_entry(slug: &str) -> NewTimeEntry {
    NewTimeEntry {
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        task_id: None,
        started_at: Utc::now(),
        note: None,
    }
}

#[test]
fn test_create_find_running() {
    let s = make_store();
    s.create(new_entry("e1")).expect("create");
    let running = s.find_running().expect("find running").expect("some");
    assert_eq!(running.slug, "e1");
}

#[test]
fn test_stop() {
    let s = make_store();
    s.create(new_entry("e2")).expect("create");
    let stopped = s.stop("e2", Utc::now()).expect("stop");
    assert!(stopped.ended_at.is_some());
    assert!(s.find_running().expect("running").is_none());
}

#[test]
fn test_create_second_running_rejected_by_db() {
    let s = make_store();
    s.create(new_entry("first-runner")).expect("first create");
    let err = s
        .create(new_entry("second-runner"))
        .expect_err("second concurrent runner must fail");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected unique-index failure, got: {err}"
    );
}
