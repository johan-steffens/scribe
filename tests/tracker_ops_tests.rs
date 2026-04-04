//! Unit tests for [`crate::ops::tracker::TrackerOps`].

use scribe::domain::ProjectId;
use scribe::ops::tracker::StartTimer;
use scribe::testing::tracker_ops;

use tracker_ops::ops as make_ops;

#[test]
fn test_start_timer_creates_entry() {
    let ops = make_ops();
    let entry = ops
        .start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .expect("start");
    assert!(entry.ended_at.is_none());
    assert!(entry.slug.starts_with("quick-capture-entry-"));
}

#[test]
fn test_start_timer_blocked_when_running() {
    let ops = make_ops();
    ops.start_timer(StartTimer {
        project_slug: "quick-capture".to_owned(),
        project_id: ProjectId(1),
        task_id: None,
        note: None,
    })
    .expect("first start");

    let err = ops
        .start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .unwrap_err();
    assert!(err.to_string().contains("already running"));
}

#[test]
fn test_stop_timer() {
    let ops = make_ops();
    ops.start_timer(StartTimer {
        project_slug: "quick-capture".to_owned(),
        project_id: ProjectId(1),
        task_id: None,
        note: None,
    })
    .expect("start");
    let stopped = ops.stop_timer().expect("stop");
    assert!(stopped.ended_at.is_some());
}

#[test]
fn test_stop_timer_when_none_running() {
    let ops = make_ops();
    let err = ops.stop_timer().unwrap_err();
    assert!(err.to_string().contains("no timer"));
}

#[test]
fn test_timer_status_none_when_idle() {
    let ops = make_ops();
    assert!(ops.timer_status().expect("status").is_none());
}

#[test]
fn test_timer_status_returns_elapsed() {
    let ops = make_ops();
    ops.start_timer(StartTimer {
        project_slug: "quick-capture".to_owned(),
        project_id: ProjectId(1),
        task_id: None,
        note: None,
    })
    .expect("start");
    let (_, elapsed) = ops.timer_status().expect("status").expect("running");
    assert!(elapsed.num_seconds() >= 0);
}

#[test]
fn test_list_recent_returns_entries() {
    let ops = make_ops();
    ops.start_timer(StartTimer {
        project_slug: "quick-capture".to_owned(),
        project_id: ProjectId(1),
        task_id: None,
        note: None,
    })
    .expect("start");
    ops.stop_timer().expect("stop");
    let recent = ops.list_recent(10).expect("list_recent");
    assert!(!recent.is_empty());
}

#[test]
fn test_update_note_changes_note() {
    let ops = make_ops();
    let entry = ops
        .start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .expect("start");
    let updated = ops
        .update_note(&entry.slug, Some("My note".to_owned()))
        .expect("update note");
    assert_eq!(updated.note.as_deref(), Some("My note"));
}

#[test]
fn test_archive_entry_archives() {
    let ops = make_ops();
    let entry = ops
        .start_timer(StartTimer {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            task_id: None,
            note: None,
        })
        .expect("start");
    ops.stop_timer().expect("stop");
    let archived = ops.archive_entry(&entry.slug).expect("archive entry");
    assert!(archived.archived_at.is_some());
}
