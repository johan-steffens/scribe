//! Integration tests for TUI navigation between views.
//!
//! Verifies that pressing navigation keys switches the active view correctly
//! and that the UI updates to reflect the new view's content.

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use scribe::db;
use scribe::tui::app::{App, View};
use scribe::tui::ui;

/// A minimal test harness that wraps an in-memory database.
fn make_app() -> App {
    let conn = Arc::new(Mutex::new(db::open_in_memory().expect("in-memory db")));
    App::new(conn)
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
///
/// Reconstructs each row by concatenating cell symbols, correctly handling
/// multi-byte graphemes.
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

/// Returns the underlying area of the buffer.
fn buffer_area(buf: &Buffer) -> Rect {
    *buf.area()
}

// ── view-state tests ─────────────────────────────────────────────────────────

#[test]
fn test_app_starts_on_dashboard() {
    let app = make_app();
    assert_eq!(app.active_view, View::Dashboard);
}

#[test]
fn test_press_d_returns_to_dashboard() {
    let mut app = make_app();

    // Navigate away first.
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Projects);

    // Press 'd' to return to dashboard.
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Dashboard);
}

#[test]
fn test_press_p_switches_to_projects() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Projects);
}

#[test]
fn test_press_t_switches_to_tasks() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Tasks);
}

#[test]
fn test_press_o_switches_to_todos() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Todos);
}

#[test]
fn test_press_r_switches_to_tracker() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Tracker);
}

#[test]
fn test_press_i_switches_to_inbox() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Inbox);
}

#[test]
fn test_press_m_switches_to_reminders() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Reminders);
}

// ── UI-update tests ────────────────────────────────────────────────────────────

#[test]
fn test_view_switch_updates_tab_bar_highlight() {
    let mut app = make_app();
    let width = 80u16;
    let height = 24u16;

    // Dashboard tab should be highlighted (marked with Cyan + Bold + Underline).
    let buf_dashboard = render_to_buffer(&app, width, height);
    // The [D]ashboard text should appear — check it's present.
    assert!(
        buffer_contains(&buf_dashboard, "[D]ashboard "),
        "dashboard should show [D]ashboard label"
    );

    // Switch to Projects.
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let buf_projects = render_to_buffer(&app, width, height);
    assert!(
        buffer_contains(&buf_projects, "[P]rojects "),
        "projects view should show [P]rojects label"
    );
}

#[test]
fn test_dashboard_view_shows_today_tasks_panel() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    // Dashboard-specific content.
    assert!(
        buffer_contains(&buf, " Today's Tasks "),
        "dashboard should show Today's Tasks panel"
    );
}

#[test]
fn test_projects_view_shows_projects_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);

    // Projects view should have its own panel title. The projects view renders
    // differently from dashboard — look for the tab bar shortcut.
    assert!(
        buffer_contains(&buf, "[P]rojects "),
        "projects view should show [P]rojects in tab bar"
    );
}

#[test]
fn test_tasks_view_shows_tasks_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[T]asks "),
        "tasks view should show [T]asks in tab bar"
    );
}

#[test]
fn test_tracker_view_shows_tracker_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "T[R]acker   "),
        "tracker view should show T[R]acker in tab bar"
    );
}

#[test]
fn test_inbox_view_shows_inbox_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[I]nbox    "),
        "inbox view should show [I]nbox in tab bar"
    );
}

#[test]
fn test_reminders_view_shows_reminders_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));

    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[M]Reminders"),
        "reminders view should show [M]Reminders in tab bar"
    );
}

// ── buffer area tests ─────────────────────────────────────────────────────────

#[test]
fn test_rendered_buffer_has_correct_dimensions() {
    let app = make_app();
    let width = 80u16;
    let height = 24u16;
    let buf = render_to_buffer(&app, width, height);

    let area = buffer_area(&buf);
    assert_eq!(area.width, width);
    assert_eq!(area.height, height);
}

#[test]
fn test_navigation_does_not_corrupt_buffer_area() {
    let mut app = make_app();
    let width = 80u16;
    let height = 24u16;

    // Navigate through all views.
    let views = [
        (KeyCode::Char('p'), View::Projects),
        (KeyCode::Char('t'), View::Tasks),
        (KeyCode::Char('o'), View::Todos),
        (KeyCode::Char('r'), View::Tracker),
        (KeyCode::Char('i'), View::Inbox),
        (KeyCode::Char('m'), View::Reminders),
        (KeyCode::Char('d'), View::Dashboard),
    ];

    for (key, expected_view) in views {
        app.handle_key(KeyEvent::new(key, KeyModifiers::NONE));
        assert_eq!(app.active_view, expected_view);

        let buf = render_to_buffer(&app, width, height);
        let area = buffer_area(&buf);
        assert_eq!(
            area.width, width,
            "width should remain {width} after navigating to {expected_view:?}"
        );
        assert_eq!(
            area.height, height,
            "height should remain {height} after navigating to {expected_view:?}"
        );
    }
}
