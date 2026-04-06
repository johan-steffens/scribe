//! MCP server integration tests.
//!
//! Tests the MCP server by exercising the ops layer that the MCP tools wrap.
//! Since the MCP tool methods are private (dispatched via the tool router),
//! we test the underlying ops directly to verify the functionality that
//! the MCP layer exposes.
//!
//! Full MCP protocol tests would require spawning the `scribe mcp` subprocess
//! and sending JSON-RPC messages via stdio, which is better suited for
//! integration tests with a real MCP client.

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use scribe::db;
use scribe::domain::{NewProject, ProjectStatus, TaskPriority, TaskStatus};
use scribe::ops::inbox::ProcessAction;
use scribe::ops::reminders::CreateReminder;
use scribe::ops::tasks::CreateTask;
use scribe::ops::tracker::StartTimer;
use scribe::ops::{InboxOps, ProjectOps, ReminderOps, TaskOps, TodoOps, TrackerOps};

/// A test harness that wraps an in-memory database and all ops structs.
#[derive(Debug)]
struct TestContext {
    pub projects: ProjectOps,
    pub tasks: TaskOps,
    pub todos: TodoOps,
    pub tracker: TrackerOps,
    pub inbox: InboxOps,
    pub reminders: ReminderOps,
}

impl TestContext {
    fn new() -> Self {
        let conn = Arc::new(Mutex::new(db::open_in_memory().expect("in-memory db")));
        let projects = ProjectOps::new(&conn);
        let tasks = TaskOps::new(Arc::clone(&conn));
        let todos = TodoOps::new(Arc::clone(&conn));
        let tracker = TrackerOps::new(Arc::clone(&conn));
        let inbox = InboxOps::new(&conn);
        let reminders = ReminderOps::new(Arc::clone(&conn));
        Self {
            projects,
            tasks,
            todos,
            tracker,
            inbox,
            reminders,
        }
    }
}

impl Default for TestContext {
    fn default() -> Self {
        Self::new()
    }
}

// ── Project ops tests ─────────────────────────────────────────────────────────

#[test]
fn test_project_ops_list_includes_quick_capture() {
    let ctx = TestContext::new();
    // A new database starts with the quick-capture project.
    let result = ctx
        .projects
        .list_projects(None, false)
        .expect("list should work");
    assert!(
        !result.is_empty(),
        "expected non-empty list, got {result:?}"
    );
    assert!(
        result.iter().any(|p| p.slug == "quick-capture"),
        "expected quick-capture project, got {result:?}"
    );
}

#[test]
fn test_project_ops_create_and_list() {
    let ctx = TestContext::new();

    ctx.projects
        .create_project(NewProject {
            slug: "test-proj".into(),
            name: "Test Project".into(),
            description: Some("A test project".into()),
            status: ProjectStatus::Active,
        })
        .expect("create should work");

    let result = ctx
        .projects
        .list_projects(None, false)
        .expect("list should work");
    assert!(
        result.iter().any(|p| p.slug == "test-proj"),
        "expected test-proj in list, got {result:?}"
    );
}

#[test]
fn test_project_ops_create_duplicate_fails() {
    let ctx = TestContext::new();

    ctx.projects
        .create_project(NewProject {
            slug: "dup-proj".into(),
            name: "First".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("first create should work");

    let result = ctx.projects.create_project(NewProject {
        slug: "dup-proj".into(),
        name: "Second".into(),
        description: None,
        status: ProjectStatus::Active,
    });

    assert!(
        result.is_err(),
        "expected error for duplicate, got: {result:?}"
    );
}

#[test]
fn test_project_ops_archive_and_restore() {
    let ctx = TestContext::new();

    ctx.projects
        .create_project(NewProject {
            slug: "archive-proj".into(),
            name: "To Be Archived".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("create should work");

    let archived = ctx
        .projects
        .archive_project("archive-proj")
        .expect("archive should work");
    assert!(
        archived.archived_at.is_some(),
        "expected archived_at to be set"
    );

    let restored = ctx
        .projects
        .restore_project("archive-proj")
        .expect("restore should work");
    assert!(
        restored.archived_at.is_none(),
        "expected archived_at to be cleared"
    );
}

#[test]
fn test_project_ops_not_found() {
    let ctx = TestContext::new();
    let result = ctx.projects.archive_project("nonexistent");
    assert!(result.is_err(), "expected error for nonexistent project");
}

// ── Task ops tests ───────────────────────────────────────────────────────────

#[test]
fn test_task_ops_list_empty() {
    let ctx = TestContext::new();
    let result = ctx
        .tasks
        .list_tasks(None, None, None, false)
        .expect("list should work");
    assert!(result.is_empty(), "expected empty list, got {result:?}");
}

#[test]
fn test_task_ops_create_with_project() {
    let ctx = TestContext::new();

    let project = ctx
        .projects
        .create_project(NewProject {
            slug: "task-test-proj".into(),
            name: "Task Test".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("project should be created");

    let task = ctx
        .tasks
        .create_task(CreateTask {
            project_slug: project.slug.clone(),
            project_id: project.id,
            title: "My task".into(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::High,
            due_date: None,
        })
        .expect("task should be created");

    assert_eq!(task.title.as_str(), "My task");
    assert_eq!(task.priority, TaskPriority::High);
}

#[test]
fn test_task_ops_create_without_project_uses_quick_capture() {
    let ctx = TestContext::new();

    // The quick-capture project should exist by default in a new DB.
    let qc = ctx
        .projects
        .get_project("quick-capture")
        .expect("get project should work");
    assert!(qc.is_some(), "quick-capture project should exist");

    let task = ctx
        .tasks
        .create_task(CreateTask {
            project_slug: "quick-capture".into(),
            project_id: qc.unwrap().id,
            title: "Task without explicit project".into(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
        })
        .expect("task should be created");

    assert_eq!(task.title.as_str(), "Task without explicit project");
}

#[test]
fn test_task_ops_invalid_priority() {
    let result = TaskPriority::from_str("invalid");
    assert!(result.is_err(), "expected error for invalid priority");
}

// ── Todo ops tests ──────────────────────────────────────────────────────────

#[test]
fn test_todo_ops_list_empty() {
    let ctx = TestContext::new();
    let result = ctx
        .todos
        .list(None, false, false)
        .expect("list should work");
    assert!(result.is_empty(), "expected empty list, got {result:?}");
}

#[test]
fn test_todo_ops_create_and_mark_done() {
    let ctx = TestContext::new();

    let project = ctx
        .projects
        .create_project(NewProject {
            slug: "todo-test-proj".into(),
            name: "Todo Test".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("project should be created");

    let todo = ctx
        .todos
        .create(&project.slug, "My todo")
        .expect("todo should be created");

    assert_eq!(todo.title.as_str(), "My todo");
    assert!(!todo.done, "todo should not be done yet");

    let done = ctx
        .todos
        .mark_done(&todo.slug)
        .expect("mark_done should work");
    assert!(done.done, "todo should be done");
}

#[test]
fn test_todo_ops_archive() {
    let ctx = TestContext::new();

    let project = ctx
        .projects
        .create_project(NewProject {
            slug: "archive-todo-proj".into(),
            name: "Archive Todo Test".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("project should be created");

    let todo = ctx
        .todos
        .create(&project.slug, "Todo to archive")
        .expect("todo should be created");

    let archived = ctx.todos.archive(&todo.slug).expect("archive should work");
    assert!(
        archived.archived_at.is_some(),
        "expected archived_at to be set"
    );
}

// ── Capture/Inbox ops tests ─────────────────────────────────────────────────

#[test]
fn test_inbox_ops_capture_basic() {
    let ctx = TestContext::new();

    let item = ctx
        .inbox
        .capture("My first capture")
        .expect("capture should work");

    assert_eq!(item.body.as_str(), "My first capture");
    assert!(
        item.slug.starts_with("capture-"),
        "expected auto-generated slug"
    );
}

#[test]
fn test_inbox_ops_capture_empty_fails() {
    let ctx = TestContext::new();
    let result = ctx.inbox.capture("");
    assert!(result.is_err(), "expected error for empty capture");
}

#[test]
fn test_inbox_ops_list_after_capture() {
    let ctx = TestContext::new();

    ctx.inbox
        .capture("Inbox item")
        .expect("capture should work");

    let items = ctx.inbox.list(false).expect("list should work");
    assert_eq!(items.len(), 1, "expected 1 item, got {items:?}");
    assert_eq!(items[0].body.as_str(), "Inbox item");
}

#[test]
fn test_inbox_ops_process_discard() {
    let ctx = TestContext::new();

    let item = ctx
        .inbox
        .capture("Item to discard")
        .expect("capture should work");

    let processed = ctx
        .inbox
        .process(&item.slug, ProcessAction::Discard)
        .expect("process should work");

    assert!(processed.processed, "expected item to be processed");
}

#[test]
fn test_inbox_ops_process_to_task_requires_project() {
    let ctx = TestContext::new();

    let item = ctx
        .inbox
        .capture("Item to convert")
        .expect("capture should work");

    let result = ctx.inbox.process(
        &item.slug,
        ProcessAction::ConvertToTask {
            project_slug: "nonexistent".into(),
            title: None,
            priority: None,
        },
    );

    assert!(result.is_err(), "expected error for nonexistent project");
}

// ── Timer/Tracker ops tests ─────────────────────────────────────────────────

#[test]
fn test_tracker_ops_timer_status_none() {
    let ctx = TestContext::new();
    let result = ctx
        .tracker
        .timer_status()
        .expect("timer_status should work");
    assert!(
        result.is_none(),
        "expected no timer running, got: {result:?}"
    );
}

#[test]
fn test_tracker_ops_start_and_stop_timer() {
    let ctx = TestContext::new();

    let project = ctx
        .projects
        .create_project(NewProject {
            slug: "timer-test-proj".into(),
            name: "Timer Test".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("project should be created");

    let started = ctx
        .tracker
        .start_timer(StartTimer {
            project_slug: project.slug.clone(),
            project_id: project.id,
            task_id: None,
            note: Some("Test timer".into()),
        })
        .expect("start_timer should work");

    assert_eq!(started.project_id, project.id);

    let stopped = ctx.tracker.stop_timer().expect("stop_timer should work");
    assert!(stopped.ended_at.is_some(), "expected timer to have ended");
}

#[test]
#[allow(
    deprecated,
    reason = "testing legacy report method for MCP compatibility"
)]
fn test_tracker_ops_report_empty() {
    let ctx = TestContext::new();

    let now = Utc::now();
    let result = ctx
        .tracker
        .report(None, now - Duration::hours(1), now)
        .expect("report should work");

    assert!(result.is_empty(), "expected empty report, got: {result:?}");
}

// ── Reminder ops tests ───────────────────────────────────────────────────────

#[test]
fn test_reminder_ops_create() {
    let ctx = TestContext::new();

    let project = ctx
        .projects
        .create_project(NewProject {
            slug: "reminder-test-proj".into(),
            name: "Reminder Test".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("project should be created");

    let reminder = ctx
        .reminders
        .create(CreateReminder {
            project_slug: project.slug.clone(),
            task_slug: None,
            remind_at: Utc::now() + Duration::hours(1),
            message: Some("Test reminder".into()),
            persistent: false,
        })
        .expect("reminder should be created");

    assert_eq!(reminder.message.as_deref(), Some("Test reminder"));
}

#[test]
fn test_reminder_ops_archive() {
    let ctx = TestContext::new();

    let project = ctx
        .projects
        .create_project(NewProject {
            slug: "archive-reminder-proj".into(),
            name: "Archive Reminder Test".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("project should be created");

    let reminder = ctx
        .reminders
        .create(CreateReminder {
            project_slug: project.slug.clone(),
            task_slug: None,
            remind_at: Utc::now() + Duration::hours(1),
            message: Some("To be archived".into()),
            persistent: false,
        })
        .expect("reminder should be created");

    let archived = ctx
        .reminders
        .archive(&reminder.slug)
        .expect("archive should work");

    assert!(
        archived.archived_at.is_some(),
        "expected archived_at to be set"
    );
}

// ── Domain parsing tests ────────────────────────────────────────────────────

#[test]
fn test_domain_task_priority_parsing() {
    assert_eq!(TaskPriority::from_str("low").unwrap(), TaskPriority::Low);
    assert_eq!(
        TaskPriority::from_str("medium").unwrap(),
        TaskPriority::Medium
    );
    assert_eq!(TaskPriority::from_str("high").unwrap(), TaskPriority::High);
    assert_eq!(
        TaskPriority::from_str("urgent").unwrap(),
        TaskPriority::Urgent
    );
    assert!(
        TaskPriority::from_str("invalid").is_err(),
        "expected error for invalid priority"
    );
}

#[test]
fn test_domain_task_status_parsing() {
    assert_eq!(TaskStatus::from_str("todo").unwrap(), TaskStatus::Todo);
    assert_eq!(
        TaskStatus::from_str("in_progress").unwrap(),
        TaskStatus::InProgress
    );
    assert_eq!(TaskStatus::from_str("done").unwrap(), TaskStatus::Done);
    assert_eq!(
        TaskStatus::from_str("cancelled").unwrap(),
        TaskStatus::Cancelled
    );
    assert!(
        TaskStatus::from_str("invalid").is_err(),
        "expected error for invalid status"
    );
}

#[test]
fn test_domain_project_status_parsing() {
    assert_eq!(
        ProjectStatus::from_str("active").unwrap(),
        ProjectStatus::Active
    );
    assert_eq!(
        ProjectStatus::from_str("paused").unwrap(),
        ProjectStatus::Paused
    );
    assert_eq!(
        ProjectStatus::from_str("completed").unwrap(),
        ProjectStatus::Completed
    );
    assert!(
        ProjectStatus::from_str("invalid").is_err(),
        "expected error for invalid status"
    );
}
