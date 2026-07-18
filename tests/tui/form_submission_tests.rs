//! Integration tests for adding projects and tasks via TUI forms.
//!
//! Uses `TestBackend` to verify that the buffer state changes after
//! "typing" field values and submitting the form.

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use scribe::db;
use scribe::tui::app::{App, View};
use scribe::tui::ui;

/// A minimal test harness that wraps an in-memory database.
fn make_app() -> App {
    let conn = Arc::new(Mutex::new(db::open_in_memory().expect("in-memory db")));
    App::new(conn, None)
}

/// Renders `app` into a `Terminal<TestBackend>` and returns a cloned buffer for inspection.
fn render_to_buffer(app: &App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal with test backend");
    terminal
        .draw(|frame| ui::draw(frame, app))
        .expect("draw should succeed");
    terminal.backend().buffer().clone()
}

/// Returns true if `needle` appears anywhere in the buffer.
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

// ── Form creation helpers ────────────────────────────────────────────────────

/// Opens the "new project" form by pressing 'N' in the projects view.
fn open_new_project_form(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE));
}

/// Opens the "new task" form by pressing 'N' in the tasks view.
fn open_new_task_form(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE));
}

/// Simulates typing a string into the current form field.
fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

// ── New Project Form Tests ───────────────────────────────────────────────────

#[test]
fn test_press_n_opens_new_project_form() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // After pressing 'n', the view should still be Projects
    // and we should be in a modal state.
    assert_eq!(app.active_view, View::Projects);

    // The form should render in the buffer - check for "New Project" title.
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "New Project"),
        "pressing 'n' should open a form with 'New Project' title"
    );
}

#[test]
fn test_project_form_has_slug_and_name_fields() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Slug"),
        "form should have a 'Slug' field"
    );
    assert!(
        buffer_contains(&buf, "Name"),
        "form should have a 'Name' field"
    );
}

#[test]
fn test_project_form_tab_navigates_fields() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // Type in first field (Slug).
    type_text(&mut app, "my-project");

    // Tab to next field.
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Type in second field (Name).
    type_text(&mut app, "My Project");

    // The form should still be open.
    assert_eq!(app.active_view, View::Projects);
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "New Project"),
        "form should remain open after tabbing"
    );
}

#[test]
fn test_project_form_escape_cancels() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    // After escape, the form should be closed (no "New Project" in buffer).
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        !buffer_contains(&buf, "New Project"),
        "pressing Escape should cancel and close the form"
    );
}

#[test]
fn test_project_form_enter_on_last_field_submits() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // Fill in Slug field.
    type_text(&mut app, "test-proj");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Fill in Name field.
    type_text(&mut app, "Test Project");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Fill in Description field (optional, last field).
    type_text(&mut app, "A test project");

    // Submit.
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // After submission, form should be closed.
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        !buffer_contains(&buf, "New Project"),
        "submitted form should be closed"
    );
}

#[test]
fn test_project_form_creates_project_in_db() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // Fill in Slug field.
    type_text(&mut app, "new-project");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Fill in Name field.
    type_text(&mut app, "New Project");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Fill in Description field.
    type_text(&mut app, "Description here");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Refresh to see the new project.
    app.refresh();

    // Check that the project was created.
    let project_exists = app.projects.items.iter().any(|p| p.slug == "new-project");
    assert!(
        project_exists,
        "project 'new-project' should exist in the projects list"
    );
}

// ── New Task Form Tests ──────────────────────────────────────────────────────

#[test]
fn test_press_n_opens_new_task_form() {
    let mut app = make_app();
    open_new_task_form(&mut app);

    // After pressing 'n', the view should still be Tasks.
    assert_eq!(app.active_view, View::Tasks);

    // The form should render in the buffer - check for "New Task" title.
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "New Task"),
        "pressing 'n' should open a form with 'New Task' title"
    );
}

#[test]
fn test_task_form_has_required_fields() {
    let mut app = make_app();
    open_new_task_form(&mut app);

    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "Title"),
        "form should have a 'Title' field"
    );
    assert!(
        buffer_contains(&buf, "Project"),
        "form should have a 'Project' field"
    );
    assert!(
        buffer_contains(&buf, "Priority"),
        "form should have a 'Priority' field"
    );
    assert!(
        buffer_contains(&buf, "Due"),
        "form should have a 'Due' field"
    );
}

#[test]
fn test_task_form_escape_cancels() {
    let mut app = make_app();
    open_new_task_form(&mut app);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    // After escape, the form should be closed.
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        !buffer_contains(&buf, "New Task"),
        "pressing Escape should cancel and close the form"
    );
}

#[test]
fn test_task_form_creates_task_in_db() {
    let mut app = make_app();
    open_new_task_form(&mut app);

    // Fill in Title field.
    type_text(&mut app, "Test task");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Skip through remaining fields to the last one.
    for _ in 0..3 {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }

    // Submit.
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Refresh to see the new task.
    app.refresh();

    // Check that a task was created.
    let task_exists = app.tasks.items.iter().any(|t| t.title == "Test task");
    assert!(
        task_exists,
        "task 'Test task' should exist in the tasks list"
    );
}

#[test]
fn test_task_form_creates_with_selected_priority() {
    use scribe::domain::TaskPriority;

    let mut app = make_app();
    open_new_task_form(&mut app);

    type_text(&mut app, "Urgent ship");
    // Title → Project → Priority → Due
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    // Default selected is medium (index 1); j twice → urgent (index 3).
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    // Advance to Due (last field) and submit empty due.
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    app.refresh();
    let task = app
        .tasks
        .items
        .iter()
        .find(|t| t.title == "Urgent ship")
        .expect("task created");
    assert_eq!(task.priority, TaskPriority::Urgent);
}

#[test]
fn test_task_form_creates_with_due_date() {
    use chrono::NaiveDate;

    let mut app = make_app();
    open_new_task_form(&mut app);

    type_text(&mut app, "Dated task");
    // Title → Project → Priority → Due
    for _ in 0..3 {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    type_text(&mut app, "2026-08-01");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    app.refresh();
    let task = app
        .tasks
        .items
        .iter()
        .find(|t| t.title == "Dated task")
        .expect("task created");
    assert_eq!(
        task.due_date,
        Some(NaiveDate::from_ymd_opt(2026, 8, 1).expect("date"))
    );
}

#[test]
fn test_edit_task_form_updates_priority_and_due() {
    use chrono::NaiveDate;
    use scribe::domain::{ProjectId, TaskPriority};
    use scribe::ops::tasks::{CreateTask, TaskOps};

    let conn = Arc::new(Mutex::new(db::open_in_memory().expect("in-memory db")));
    TaskOps::new(Arc::clone(&conn))
        .create_task(CreateTask {
            project_slug: "quick-capture".to_owned(),
            project_id: ProjectId(1),
            title: "Edit me".to_owned(),
            description: None,
            status: scribe::domain::TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
        })
        .expect("create task");

    let mut app = App::new(conn, None);
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    assert!(buffer_contains(&buf, "Edit Task"), "edit form should open");
    assert!(
        buffer_contains(&buf, "Priority"),
        "edit form should include Priority"
    );
    assert!(buffer_contains(&buf, "Due"), "edit form should include Due");

    // Keep title; Tab to Priority → High (index 2; medium is 1, so j once).
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    // Tab to Due and set a date.
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    type_text(&mut app, "2026-09-15");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    app.refresh();
    let task = app
        .tasks
        .items
        .iter()
        .find(|t| t.title == "Edit me")
        .expect("task still listed");
    assert_eq!(task.priority, TaskPriority::High);
    assert_eq!(
        task.due_date,
        Some(NaiveDate::from_ymd_opt(2026, 9, 15).expect("date"))
    );
}

#[test]
fn test_edit_todo_form_moves_project() {
    use scribe::domain::{NewProject, ProjectStatus};
    use scribe::ops::{ProjectOps, TodoOps};

    let conn = Arc::new(Mutex::new(db::open_in_memory().expect("in-memory db")));
    ProjectOps::new(&conn)
        .create_project(NewProject {
            slug: "work".to_owned(),
            name: "Work".to_owned(),
            description: None,
            status: ProjectStatus::Active,
        })
        .expect("create work project");
    TodoOps::new(Arc::clone(&conn))
        .create("quick-capture", "Rehome me")
        .expect("create todo");

    let mut app = App::new(conn, None);

    // Todos view → edit selected todo.
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);
    assert!(buffer_contains(&buf, "Edit Todo"), "edit form should open");

    // Title field is focused; tab to Project select (pre-filled on quick-capture).
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    // Active projects list as quick-capture then work; j selects work.
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    app.refresh();
    let todo = app
        .todos
        .items
        .iter()
        .find(|t| t.title == "Rehome me")
        .expect("todo still listed");
    let work_id = app
        .projects
        .items
        .iter()
        .find(|p| p.slug == "work")
        .expect("work project")
        .id;
    assert_eq!(
        todo.project_id, work_id,
        "edit-todo Project field must call move_project (got project_id {:?})",
        todo.project_id
    );
}

// ── Form Field Navigation Tests ───────────────────────────────────────────────

#[test]
fn test_shift_tab_goes_to_previous_field() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // Type in first field.
    type_text(&mut app, "slug-value");

    // Tab forward.
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Shift-Tab back.
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));

    // The form should still be open.
    assert_eq!(app.active_view, View::Projects);
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "New Project"),
        "Shift-Tab should navigate to previous field"
    );
}

#[test]
fn test_form_with_multiple_fields_submits_on_enter_on_last_field() {
    // Navigate to todos view.
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    // Open the new todo form.
    app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE));

    // The form should show "New Todo".
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "New Todo"),
        "pressing 'N' in todos view should open a 'New Todo' form"
    );

    // Type a todo title.
    type_text(&mut app, "Buy groceries");

    // Tab to the Project field (last field).
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    // Now submit with Enter on the last field.
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // The form should be closed after submission.
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        !buffer_contains(&buf, "New Todo"),
        "submitted todo form should be closed"
    );
}

// ── Buffer State After Form Submission Tests ─────────────────────────────────

#[test]
fn test_buffer_shows_projects_list_after_form_close() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // Fill and submit.
    type_text(&mut app, "buf-test");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    type_text(&mut app, "Buffer Test");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    type_text(&mut app, "");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // The projects view should now be visible again.
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "[P]rojects "),
        "projects view should be visible after form submission"
    );
}

#[test]
fn test_buffer_shows_tasks_list_after_form_close() {
    let mut app = make_app();
    open_new_task_form(&mut app);

    // Fill and submit.
    type_text(&mut app, "Another task");
    for _ in 0..3 {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // The tasks view should now be visible again.
    let buf = render_to_buffer(&app, 80, 24);
    assert!(
        buffer_contains(&buf, "[T]asks "),
        "tasks view should be visible after form submission"
    );
}

// ── Form Validation Tests ────────────────────────────────────────────────────

#[test]
fn test_form_handles_empty_submission() {
    let mut app = make_app();
    // Navigate to todos and open new todo form.
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE));

    // Submit without typing anything.
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Form should either submit with empty values or stay open for validation.
    // At minimum, the app should not panic.
}

// ── Multi-field Form Navigation Stress Test ─────────────────────────────────

#[test]
fn test_form_tab_cycles_through_all_fields() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // Tab through all fields multiple times.
    for _ in 0..5 {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(
            app.active_view,
            View::Projects,
            "form should remain open while tabbing through fields"
        );
    }
}

#[test]
fn test_form_shift_tab_cycles_back_through_all_fields() {
    let mut app = make_app();
    open_new_project_form(&mut app);

    // Go forward first.
    for _ in 0..3 {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }

    // Then go back.
    for _ in 0..3 {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));
        assert_eq!(
            app.active_view,
            View::Projects,
            "form should remain open while shift-tabbing through fields"
        );
    }
}
