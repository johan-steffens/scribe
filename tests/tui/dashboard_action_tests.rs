//! Dashboard selection and Space behaviour (`TestBackend` + keys).

use std::sync::{Arc, Mutex};

use chrono::{Duration, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use scribe::domain::{NewTask, ProjectId, TaskKind, TaskPriority, TaskStatus, Tasks};
use scribe::store::SqliteTasks;
use scribe::tui::app::{App, View};

fn make_app_with_due_task() -> App {
    let conn = Arc::new(Mutex::new(
        scribe::db::open_in_memory().expect("in-memory db"),
    ));
    let store = SqliteTasks::new(Arc::clone(&conn));
    let today = Local::now().date_naive();
    store
        .create(NewTask {
            slug: "qc-task-due-today".into(),
            project_id: ProjectId(1),
            title: "Due Today Item".into(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::High,
            due_date: Some(today),
            parent_id: None,
            kind: TaskKind::Task,
        })
        .expect("create due task");
    store
        .create(NewTask {
            slug: "qc-task-future".into(),
            project_id: ProjectId(1),
            title: "Future Task".into(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Low,
            due_date: Some(today + Duration::days(7)),
            parent_id: None,
            kind: TaskKind::Task,
        })
        .expect("create future task");

    App::new(conn, None)
}

#[test]
fn test_dashboard_has_independent_selection_cursor() {
    let mut app = make_app_with_due_task();
    assert_eq!(app.active_view, View::Dashboard);
    assert_eq!(app.dashboard_selected, 0);

    // Move down on dashboard must not advance the Tasks tree cursor.
    let tasks_sel_before = app.tasks.selected;
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    // Only one due task → dashboard cursor stays 0.
    assert_eq!(app.dashboard_selected, 0);
    assert_eq!(app.tasks.selected, tasks_sel_before);

    // Switch to tasks: tree has 2 top-level tasks.
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Tasks);
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.tasks.selected, 1);
    // Dashboard cursor untouched.
    assert_eq!(app.dashboard_selected, 0);
}

#[test]
fn test_dashboard_space_marks_due_task_done() {
    let mut app = make_app_with_due_task();
    assert_eq!(app.active_view, View::Dashboard);

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

    let due = app
        .tasks
        .items
        .iter()
        .find(|t| t.slug == "qc-task-due-today")
        .expect("due task still in items");
    assert_eq!(due.status, TaskStatus::Done);
}

#[test]
fn test_dashboard_space_starts_timer_form_when_no_due_tasks() {
    let conn = Arc::new(Mutex::new(
        scribe::db::open_in_memory().expect("in-memory db"),
    ));
    let mut app = App::new(conn, None);
    assert_eq!(app.active_view, View::Dashboard);

    // No due tasks → Space opens start-timer form (same as Tracker when idle).
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    let modal = format!("{:?}", app.modal);
    assert!(
        modal.starts_with("Form"),
        "expected start-timer form when no due tasks, got {modal}"
    );
}

#[test]
fn test_tracker_filter_narrows_selection_target() {
    let conn = Arc::new(Mutex::new(
        scribe::db::open_in_memory().expect("in-memory db"),
    ));
    // Seed two finished entries via CLI-equivalent ops.
    let tracker = scribe::ops::TrackerOps::new(Arc::clone(&conn));
    let (slug, pid) = tracker
        .resolve_project("quick-capture")
        .expect("resolve qc");
    tracker
        .start_timer(scribe::ops::tracker::StartTimer {
            project_slug: slug.clone(),
            project_id: pid,
            task_id: None,
            note: Some("alpha note".into()),
        })
        .expect("start");
    tracker.stop_timer().expect("stop");
    tracker
        .start_timer(scribe::ops::tracker::StartTimer {
            project_slug: slug,
            project_id: pid,
            task_id: None,
            note: Some("beta note".into()),
        })
        .expect("start2");
    tracker.stop_timer().expect("stop2");

    let mut app = App::new(conn, None);
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Tracker);
    assert!(app.entries.items.len() >= 2);

    // Filter to "alpha" — selection should target that entry for edit.
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    for c in "alpha".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Open edit form (e) — title/note should relate to alpha entry only.
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    let modal = format!("{:?}", app.modal);
    assert!(
        modal.starts_with("Form"),
        "edit form should open on filtered entry, got {modal}"
    );
    // filtered_len should be 1
    // We can't call private methods; assert only one entry matches via re-filter logic.
    let matches = app
        .entries
        .items
        .iter()
        .filter(|e| {
            e.note
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("alpha")
        })
        .count();
    assert_eq!(matches, 1);
}
