//! Unit tests for [`crate::ops::tasks::TaskOps`].

use scribe::domain::{ProjectId, TaskPriority, TaskStatus};
use scribe::ops::tasks::{CreateTask, TaskOps};
use scribe::testing::task_ops;

use task_ops::ops as make_ops;

fn create(ops: &TaskOps, title: &str) -> scribe::domain::Task {
    ops.create_task(CreateTask {
        project_slug: "qc".to_owned(),
        project_id: ProjectId(1),
        title: title.to_owned(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        due_date: None,
    })
    .expect("create task")
}

#[test]
fn test_create_generates_slug() {
    let ops = make_ops();
    let t = create(&ops, "Fix Login Bug");
    assert_eq!(t.slug, "qc-task-fix-login-bug");
}

#[test]
fn test_mark_done() {
    let ops = make_ops();
    let t = create(&ops, "Finish Report");
    let done = ops.mark_done(&t.slug).expect("done");
    assert_eq!(done.status, TaskStatus::Done);
}

#[test]
fn test_list_tasks() {
    let ops = make_ops();
    create(&ops, "Task A");
    create(&ops, "Task B");
    let tasks = ops.list_tasks(None, None, None, false).expect("list");
    assert!(tasks.len() >= 2);
}

#[test]
fn test_archive_and_restore() {
    let ops = make_ops();
    let t = create(&ops, "Archive me");
    ops.archive_task(&t.slug).expect("archive");
    let active = ops.list_tasks(None, None, None, false).expect("list");
    assert!(!active.iter().any(|x| x.slug == t.slug));
    ops.restore_task(&t.slug).expect("restore");
    let active = ops.list_tasks(None, None, None, false).expect("list");
    assert!(active.iter().any(|x| x.slug == t.slug));
}
