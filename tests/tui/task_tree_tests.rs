//! Integration tests for the task tree view.
//!
//! Tests the expand/collapse functionality for the hierarchical task view,
//! including keyboard interactions (Enter, Right, Left arrows).

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use scribe::db;
use scribe::domain::{NewProject, ProjectId, ProjectStatus, TaskId, TaskPriority, TaskStatus};
use scribe::ops::projects::ProjectOps;
use scribe::ops::tasks::CreateTask;
use scribe::ops::tasks::TaskOps;
use scribe::tui::app::App;

/// A minimal test harness that wraps an in-memory database.
fn make_app_with_conn() -> (App, Arc<Mutex<rusqlite::Connection>>) {
    let conn = Arc::new(Mutex::new(db::open_in_memory().expect("in-memory db")));
    let app = App::new(Arc::clone(&conn));
    (app, conn)
}

/// Renders `app` into a `Terminal<TestBackend>` and returns a cloned buffer for inspection.
fn render_to_buffer(app: &App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal with test backend");
    terminal
        .draw(|frame| scribe::tui::ui::draw(frame, app))
        .expect("draw should succeed");
    terminal.backend().buffer().clone()
}

/// Returns true if `needle` appears anywhere in the buffer as a contiguous substring.
fn buffer_contains(buf: &Buffer, needle: &str) -> bool {
    let area = buf.area();
    let width = area.width;
    let height = area.height;

    for y in 0..height {
        let mut line = String::new();
        for x in 0..width {
            let symbol = buf[(x, y)].symbol();
            if !symbol.is_empty() {
                line.push_str(symbol);
            }
        }
        if line.contains(needle) {
            return true;
        }
    }
    false
}

/// Creates a project and returns its ID.
fn create_project(conn: &Arc<Mutex<rusqlite::Connection>>, slug: &str, name: &str) -> ProjectId {
    let ops = ProjectOps::new(conn);
    let project = ops
        .create_project(NewProject {
            slug: slug.to_string(),
            name: name.to_string(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("project creation should succeed");
    project.id
}

/// Creates a task and returns it.
fn create_task(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    project_id: ProjectId,
    project_slug: &str,
    title: &str,
    parent_id: Option<TaskId>,
) -> scribe::domain::Task {
    let ops = TaskOps::new(Arc::clone(conn));
    ops.create_task(CreateTask {
        project_slug: project_slug.to_string(),
        project_id,
        title: title.to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        due_date: None,
        parent_id,
    })
    .expect("task creation should succeed")
}

// ── Task tree expand/collapse tests ─────────────────────────────────────────

#[test]
fn test_enter_toggles_task_expansion() {
    let (mut app, conn) = make_app_with_conn();
    let project_id = create_project(&conn, "test-project", "Test Project");

    // Create a parent task
    let parent = create_task(&conn, project_id, "test-project", "Parent Task", None);
    // Create a child task
    let _child = create_task(
        &conn,
        project_id,
        "test-project",
        "Child Task",
        Some(parent.id),
    );

    // Refresh to load the tasks
    app.refresh();

    // Switch to tasks view
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // Initially, child should not be visible
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        !buffer_contains(&buf, "Child Task"),
        "child task should not be visible initially"
    );

    // Press Enter to expand
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Now child should be visible
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Child Task"),
        "child task should be visible after expand"
    );

    // Press Enter again to collapse
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        !buffer_contains(&buf, "Child Task"),
        "child task should not be visible after collapse"
    );
}

#[test]
fn test_right_arrow_expands_task() {
    let (mut app, conn) = make_app_with_conn();
    let project_id = create_project(&conn, "test-project", "Test Project");

    let parent = create_task(&conn, project_id, "test-project", "Parent Task", None);
    let _child = create_task(
        &conn,
        project_id,
        "test-project",
        "Child Task",
        Some(parent.id),
    );

    app.refresh();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // Right arrow should expand
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Child Task"),
        "child task should be visible after Right arrow"
    );
}

#[test]
fn test_left_arrow_collapses_task() {
    let (mut app, conn) = make_app_with_conn();
    let project_id = create_project(&conn, "test-project", "Test Project");

    let parent = create_task(&conn, project_id, "test-project", "Parent Task", None);
    let _child = create_task(
        &conn,
        project_id,
        "test-project",
        "Child Task",
        Some(parent.id),
    );

    app.refresh();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // First expand with Enter
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Child Task"),
        "child should be visible after expand"
    );

    // Left arrow should collapse
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        !buffer_contains(&buf, "Child Task"),
        "child should not be visible after Left arrow"
    );
}

#[test]
fn test_left_arrow_moves_to_parent() {
    let (mut app, conn) = make_app_with_conn();
    let project_id = create_project(&conn, "test-project", "Test Project");

    let parent = create_task(&conn, project_id, "test-project", "Parent Task", None);
    let _child = create_task(
        &conn,
        project_id,
        "test-project",
        "Child Task",
        Some(parent.id),
    );

    app.refresh();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // Expand the parent first
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Navigate to child (down arrow)
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

    // Now we're on the child. Left arrow on collapsed child should move to parent.
    // The parent stays expanded, so the child remains visible.
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));

    // The child should still be visible because parent is still expanded
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Child Task"),
        "child should still be visible because parent is still expanded"
    );
}

#[test]
fn test_nested_children_render_with_indentation() {
    let (mut app, conn) = make_app_with_conn();
    let project_id = create_project(&conn, "test-project", "Test Project");

    let parent = create_task(&conn, project_id, "test-project", "Parent Task", None);
    let child = create_task(
        &conn,
        project_id,
        "test-project",
        "Child Task",
        Some(parent.id),
    );
    let _grandchild = create_task(
        &conn,
        project_id,
        "test-project",
        "Grandchild Task",
        Some(child.id),
    );

    app.refresh();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // Expand parent
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    // Child should be visible but grandchild not (parent not expanded)
    assert!(
        buffer_contains(&buf, "Child Task"),
        "child should be visible"
    );
    assert!(
        !buffer_contains(&buf, "Grandchild Task"),
        "grandchild should not be visible (child not expanded)"
    );

    // Navigate to child and expand it
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // Move to child
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // Expand child

    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Grandchild Task"),
        "grandchild should be visible after expanding child"
    );
}

#[test]
fn test_tree_branch_characters_rendered() {
    let (mut app, conn) = make_app_with_conn();
    let project_id = create_project(&conn, "test-project", "Test Project");

    let parent = create_task(&conn, project_id, "test-project", "Parent Task", None);
    let _child = create_task(
        &conn,
        project_id,
        "test-project",
        "Child Task",
        Some(parent.id),
    );

    app.refresh();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // Expand
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    // The tree branch characters should appear
    assert!(
        buffer_contains(&buf, "├─") || buffer_contains(&buf, "└─"),
        "tree branch characters should be visible"
    );
}

#[test]
fn test_task_without_children_not_expanded() {
    let (mut app, conn) = make_app_with_conn();
    let project_id = create_project(&conn, "test-project", "Test Project");

    // Create a task with no children
    let _solo = create_task(&conn, project_id, "test-project", "Solo Task", None);

    app.refresh();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // Press Enter on a task without children - should toggle but nothing happens
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Should still show just the solo task
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Solo Task"),
        "solo task should be visible"
    );
}
