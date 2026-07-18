//! Unit tests for [`crate::store::task_store::SqliteTasks`].

use scribe::domain::{NewTask, ProjectId, TaskKind, TaskPatch, TaskPriority, TaskStatus, Tasks};
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
        parent_id: None,
        kind: TaskKind::Task,
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
fn test_list_includes_project_slug() {
    let s = make_store();
    s.create(new_task("qc-task-listed", "Listed"))
        .expect("create");
    let tasks = s.list(None, None, None, false).expect("list");
    let t = tasks
        .iter()
        .find(|t| t.slug == "qc-task-listed")
        .expect("listed task present");
    assert_eq!(
        t.project_slug, "quick-capture",
        "list must join project slug (not leave 'unknown')"
    );
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

#[test]
fn test_create_with_parent_id() {
    let s = make_store();
    let parent = s
        .create(new_task("parent-slug", "Parent task"))
        .expect("create parent");
    let child = s
        .create(NewTask {
            slug: "child-slug".to_owned(),
            project_id: qc_project(),
            title: "Child task".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Low,
            due_date: None,
            parent_id: Some(parent.id),
            kind: TaskKind::Task,
        })
        .expect("create child");
    assert_eq!(child.parent_id, Some(parent.id));
}

#[test]
fn test_create_checklist_item() {
    let s = make_store();
    let item = s
        .create(NewTask {
            slug: "checklist-slug".to_owned(),
            project_id: qc_project(),
            title: "Checklist item".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
            kind: TaskKind::ChecklistItem,
        })
        .expect("create checklist item");
    assert_eq!(item.kind, TaskKind::ChecklistItem);
}

#[test]
fn test_update_parent_id() {
    let s = make_store();
    let t1 = s.create(new_task("t1-parent", "Task 1")).expect("t1");
    let _t2 = s.create(new_task("t2-child", "Task 2")).expect("t2");
    let updated = s
        .update(
            "t2-child",
            TaskPatch {
                parent_id: Some(t1.id),
                ..Default::default()
            },
        )
        .expect("update parent_id");
    assert_eq!(updated.parent_id, Some(t1.id));
}

#[test]
fn test_update_kind() {
    let s = make_store();
    let t = s
        .create(new_task("kind-test", "Kind test"))
        .expect("create");
    assert_eq!(t.kind, TaskKind::Task);
    let updated = s
        .update(
            "kind-test",
            TaskPatch {
                kind: Some(TaskKind::ChecklistItem),
                ..Default::default()
            },
        )
        .expect("update kind");
    assert_eq!(updated.kind, TaskKind::ChecklistItem);
}

#[test]
fn test_clear_parent_id() {
    let s = make_store();
    let parent = s.create(new_task("par-clear", "Parent")).expect("parent");
    let child = s
        .create(NewTask {
            slug: "child-clear".to_owned(),
            project_id: qc_project(),
            title: "Child".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: Some(parent.id),
            kind: TaskKind::Task,
        })
        .expect("create child");
    assert!(child.parent_id.is_some());
    let cleared = s
        .update(
            "child-clear",
            TaskPatch {
                clear_parent_id: true,
                ..Default::default()
            },
        )
        .expect("clear parent_id");
    assert_eq!(cleared.parent_id, None);
}
