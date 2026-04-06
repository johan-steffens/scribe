//! Integration tests for the reporting system.
//!
//! These tests verify that `ReportingOps` correctly aggregates data across
//! all domain stores.

use chrono::{Duration, Utc};
use scribe::domain::{TaskPriority, TaskStatus};
use scribe::ops::tasks::CreateTask;
use scribe::ops::tracker::StartTimer;
use scribe::ops::ReportingOps;
use scribe::ops::{ProjectOps, TaskOps, TrackerOps};
use scribe::testing::db::TestDb;

/// Creates a `ReportingOps` instance backed by a fresh in-memory database.
fn reporting_ops(test_db: &TestDb) -> ReportingOps {
    ReportingOps::new(test_db.conn())
}

/// Creates a `ProjectOps` instance backed by the same database as `test_db`.
fn project_ops(test_db: &TestDb) -> ProjectOps {
    ProjectOps::new(&test_db.conn())
}

/// Creates a `TaskOps` instance backed by the same database as `test_db`.
fn task_ops(test_db: &TestDb) -> TaskOps {
    TaskOps::new(Arc::clone(&test_db.conn()))
}

/// Creates a `TrackerOps` instance backed by the same database as `test_db`.
fn tracker_ops(test_db: &TestDb) -> TrackerOps {
    TrackerOps::new(Arc::clone(&test_db.conn()))
}

use std::sync::Arc;

#[test]
fn summary_report_empty_database_has_quick_capture_project() {
    let test_db = TestDb::new();
    let ops = reporting_ops(&test_db);

    let now = Utc::now();
    let report = ops
        .summary_report(now - Duration::days(7), now)
        .expect("summary_report should succeed");

    // The quick-capture project is seeded by default
    assert_eq!(
        report.active_projects, 1,
        "quick-capture project is seeded by default"
    );
    assert_eq!(report.pending_tasks, 0, "no tasks exist yet");
    assert_eq!(report.open_todos, 0, "no todos exist yet");
    assert_eq!(report.items_in_inbox, 0, "no inbox items exist yet");
    assert_eq!(report.active_reminders, 0, "no reminders exist yet");
    assert_eq!(report.total_time_tracked, Duration::zero());
    assert_eq!(report.overdue_tasks, 0, "no tasks exist yet");
}

#[test]
fn summary_report_counts_projects_and_tasks() {
    let test_db = TestDb::new();
    let ops = reporting_ops(&test_db);
    let proj_ops = project_ops(&test_db);
    let tsk_ops = task_ops(&test_db);

    // Use the new_project helper from the testing module
    let new_proj = scribe::testing::project_ops::new_project("test-project");
    let project = proj_ops
        .create_project(new_proj)
        .expect("project creation should succeed");

    // Create a pending task
    let _pending_task = tsk_ops
        .create_task(CreateTask {
            project_slug: project.slug.clone(),
            project_id: project.id,
            title: "Test Task 1".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
        })
        .expect("task creation should succeed");

    // Create a done task
    let _done_task = tsk_ops
        .create_task(CreateTask {
            project_slug: project.slug.clone(),
            project_id: project.id,
            title: "Test Task 2".to_owned(),
            description: None,
            status: TaskStatus::Done,
            priority: TaskPriority::Medium,
            due_date: None,
        })
        .expect("done task creation should succeed");

    let now = Utc::now();
    let report = ops
        .summary_report(now - Duration::days(7), now)
        .expect("summary_report should succeed");

    // quick-capture + test-project = 2 projects
    assert_eq!(report.active_projects, 2, "two active projects exist");
    assert_eq!(
        report.pending_tasks, 1,
        "one task is pending (not done/cancelled)"
    );
    assert_eq!(report.overdue_tasks, 0, "no tasks are overdue");
}

#[test]
fn project_report_returns_project_details() {
    let test_db = TestDb::new();
    let ops = reporting_ops(&test_db);
    let proj_ops = project_ops(&test_db);

    let new_proj = scribe::testing::project_ops::new_project("report-test-project");
    let _project = proj_ops
        .create_project(new_proj)
        .expect("project creation should succeed");

    let now = Utc::now();
    let report = ops
        .project_report("report-test-project", now - Duration::days(7), now)
        .expect("project_report should succeed");

    assert_eq!(report.project.slug, "report-test-project");
    assert_eq!(report.pending_tasks.len(), 0, "no tasks yet");
    assert_eq!(report.open_todos.len(), 0, "no todos yet");
    assert!(
        report.completion_percentage < f32::EPSILON,
        "no tasks to complete, got {}",
        report.completion_percentage
    );
}

#[test]
fn task_report_returns_task_details() {
    let test_db = TestDb::new();
    let ops = reporting_ops(&test_db);
    let proj_ops = project_ops(&test_db);
    let tsk_ops = task_ops(&test_db);

    let new_proj = scribe::testing::project_ops::new_project("task-report-project");
    let project = proj_ops
        .create_project(new_proj)
        .expect("project creation should succeed");

    let task = tsk_ops
        .create_task(CreateTask {
            project_slug: project.slug.clone(),
            project_id: project.id,
            title: "My Test Task".to_owned(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
        })
        .expect("task creation should succeed");

    let report = ops
        .task_report(&task.slug)
        .expect("task_report should succeed");

    assert_eq!(report.task.slug, task.slug);
    assert_eq!(report.task.title, "My Test Task");
    assert_eq!(report.total_time, Duration::zero(), "no time entries yet");
}

#[test]
fn summary_report_with_time_tracked() {
    let test_db = TestDb::new();
    let ops = reporting_ops(&test_db);
    let proj_ops = project_ops(&test_db);
    let trk_ops = tracker_ops(&test_db);

    let new_proj = scribe::testing::project_ops::new_project("time-report-project");
    let project = proj_ops
        .create_project(new_proj)
        .expect("project creation should succeed");

    // Start and stop a timer to create a time entry
    // Start and stop a timer to create a time entry
    let _entry = trk_ops
        .start_timer(StartTimer {
            project_slug: project.slug.clone(),
            project_id: project.id,
            task_id: None,
            note: None,
        })
        .expect("timer should start");

    let now = Utc::now();
    let stopped_entry = trk_ops.stop_timer().expect("timer should stop");

    // Verify the entry has an ended_at now
    assert!(stopped_entry.ended_at.is_some(), "timer should be stopped");

    let report = ops
        .summary_report(now - Duration::days(7), now + Duration::hours(1))
        .expect("summary_report should succeed");

    assert!(
        report.total_time_tracked > Duration::zero(),
        "time was tracked"
    );
}
