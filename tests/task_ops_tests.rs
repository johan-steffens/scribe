//! Unit tests for [`crate::ops::tasks::TaskOps`].

use scribe::domain::{ProjectId, TaskPriority, TaskStatus};
use scribe::ops::tasks::{CreateTask, TaskOps};
use scribe::testing::task_ops;

use task_ops::ops as make_ops;

/// Seeded reserved project from migrations (`quick-capture`, id = 1).
const QC_SLUG: &str = "quick-capture";
const QC_ID: ProjectId = ProjectId(1);

fn create(ops: &TaskOps, title: &str) -> scribe::domain::Task {
    ops.create_task(CreateTask {
        project_slug: QC_SLUG.to_owned(),
        project_id: QC_ID,
        title: title.to_owned(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        due_date: None,
        parent_id: None,
    })
    .expect("create task")
}

#[test]
fn test_create_generates_slug() {
    let ops = make_ops();
    let t = create(&ops, "Fix Login Bug");
    assert_eq!(t.slug, "quick-capture-task-fix-login-bug");
}

#[test]
fn test_create_rejects_empty_title() {
    let ops = make_ops();
    let err = ops
        .create_task(CreateTask {
            project_slug: QC_SLUG.to_owned(),
            project_id: QC_ID,
            title: "   ".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
        })
        .expect_err("empty title must fail");
    assert!(
        err.to_string().contains("title cannot be empty"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_create_rejects_missing_project() {
    let ops = make_ops();
    let err = ops
        .create_task(CreateTask {
            project_slug: "does-not-exist".to_owned(),
            project_id: ProjectId(999),
            title: "Orphan".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
        })
        .expect_err("missing project must fail");
    assert!(
        err.to_string().contains("not found"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_create_rejects_archived_project() {
    use scribe::domain::{NewProject, ProjectStatus, Projects};
    use scribe::ops::ProjectOps;
    use scribe::store::SqliteProjects;
    use std::sync::{Arc, Mutex};

    let conn = Arc::new(Mutex::new(
        scribe::db::open_in_memory().expect("in-memory db"),
    ));
    let project_ops = ProjectOps::new(&conn);
    project_ops
        .create_project(NewProject {
            slug: "archived-proj".into(),
            name: "Archived".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("create project");
    project_ops
        .archive_project("archived-proj")
        .expect("archive");

    let ops = TaskOps::new(Arc::clone(&conn));
    let project = SqliteProjects::new(Arc::clone(&conn))
        .find_by_slug("archived-proj")
        .expect("lookup")
        .expect("exists");

    let err = ops
        .create_task(CreateTask {
            project_slug: "archived-proj".to_owned(),
            project_id: project.id,
            title: "Nope".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
        })
        .expect_err("archived project must fail");
    assert!(
        err.to_string().contains("archived"),
        "unexpected error: {err}"
    );
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
