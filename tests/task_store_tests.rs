//! Unit tests for [`crate::store::task_store::SqliteTasks`].

use scribe::domain::{NewTask, ProjectId, TaskPatch, TaskPriority, TaskStatus, Tasks};
use scribe::testing::task_store;

use task_store::store as make_store;

// The seeded quick-capture project has id=1.
fn qc_project() -> ProjectId {
    ProjectId(1)
}

fn new_task(slug: &str, title: &str) -> NewTask {
    NewTask {
        slug: slug.to_owned(),
        project_id: qc_project(),
        title: title.to_owned(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        due_date: None,
    }
}

#[test]
fn test_create_and_find() {
    let s = make_store();
    let t = s.create(new_task("qc-task-fix", "Fix it")).expect("create");
    assert_eq!(t.slug, "qc-task-fix");
    let found = s.find_by_slug("qc-task-fix").expect("find").expect("some");
    assert_eq!(found.id, t.id);
}

#[test]
fn test_archive_and_restore() {
    let s = make_store();
    s.create(new_task("t1", "T1")).expect("create");
    s.archive("t1").expect("archive");
    let tasks = s.list(None, None, None, false).expect("list");
    assert!(!tasks.iter().any(|t| t.slug == "t1"));
    s.restore("t1").expect("restore");
    let tasks = s.list(None, None, None, false).expect("list");
    assert!(tasks.iter().any(|t| t.slug == "t1"));
}

#[test]
fn test_delete() {
    let s = make_store();
    s.create(new_task("del", "Delete me")).expect("create");
    s.delete("del").expect("delete");
    assert!(s.find_by_slug("del").expect("find").is_none());
}

#[test]
fn test_update_status() {
    let s = make_store();
    s.create(new_task("upd", "Update me")).expect("create");
    let t = s
        .update(
            "upd",
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .expect("update");
    assert_eq!(t.status, TaskStatus::Done);
}

#[test]
fn test_archive_all_for_project() {
    let s = make_store();
    s.create(new_task("p-t1", "T1")).expect("t1");
    s.create(new_task("p-t2", "T2")).expect("t2");
    s.archive_all_for_project(qc_project())
        .expect("archive all");
    let active = s.list(Some(qc_project()), None, None, false).expect("list");
    assert!(active.is_empty());
}
