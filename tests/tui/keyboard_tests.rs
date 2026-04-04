//! Integration tests for keyboard interactions across all TUI views.
//!
//! Verifies that pressing navigation keys (`p`, `t`, `o`, `r`, `i`, `m`)
//! correctly switches views and that global key bindings (like `q` to quit,
//! `/` for filter mode) work consistently.

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use scribe::db;
use scribe::tui::app::{App, InputMode, View};
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

// ── Quit key tests ───────────────────────────────────────────────────────────

#[test]
fn test_press_q_sets_should_quit() {
    let mut app = make_app();
    assert!(!app.should_quit, "app should not quit initially");

    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(
        app.should_quit,
        "pressing 'q' should set should_quit to true"
    );
}

#[test]
fn test_ctrl_c_sets_should_quit() {
    let mut app = make_app();
    assert!(!app.should_quit, "app should not quit initially");

    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(
        app.should_quit,
        "pressing Ctrl-C should set should_quit to true"
    );
}

#[test]
fn test_ctrl_c_does_not_affect_view() {
    let mut app = make_app();
    let original_view = app.active_view;

    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(
        app.active_view, original_view,
        "Ctrl-C should not change the active view"
    );
}

// ── Help toggle tests ────────────────────────────────────────────────────────

#[test]
fn test_press_question_mark_toggles_help() {
    let mut app = make_app();
    assert!(!app.show_help, "help should be hidden initially");

    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.show_help, "pressing '?' should show help");

    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(!app.show_help, "pressing '?' again should hide help");
}

#[test]
fn test_escape_closes_help() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.show_help, "help should be visible after pressing '?'");

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.show_help, "pressing Escape should close help");
}

#[test]
fn test_escape_clears_last_error() {
    let mut app = make_app();
    app.last_error = Some("test error".to_owned());

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        app.last_error.is_none(),
        "pressing Escape should clear last_error"
    );
}

// ── Filter mode tests ────────────────────────────────────────────────────────

#[test]
fn test_press_slash_enters_filter_mode() {
    let mut app = make_app();
    assert_eq!(
        app.input_mode,
        InputMode::Normal,
        "should start in Normal mode"
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(
        app.input_mode,
        InputMode::Filter,
        "pressing '/' should enter Filter mode"
    );
}

#[test]
fn test_filter_mode_clears_on_escape() {
    let mut app = make_app();
    // Enter filter mode and type something.
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

    // The filter should have content.
    assert_eq!(
        app.tasks.filter, "ab",
        "filter should contain typed characters"
    );

    // Escape should clear the filter and return to normal mode.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(
        app.input_mode,
        InputMode::Normal,
        "should return to Normal mode"
    );
    assert!(
        app.tasks.filter.is_empty(),
        "filter should be cleared on Escape"
    );
}

#[test]
fn test_filter_mode_enter_exits_filter() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    assert_eq!(
        app.input_mode,
        InputMode::Filter,
        "should be in Filter mode"
    );

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.input_mode,
        InputMode::Normal,
        "pressing Enter should exit Filter mode"
    );
}

#[test]
fn test_filter_mode_backspace() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));

    assert_eq!(app.tasks.filter, "abc", "filter should contain 'abc'");

    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(
        app.tasks.filter, "ab",
        "backspace should remove last character"
    );
}

// ── Navigation key tests ──────────────────────────────────────────────────────

#[test]
fn test_press_d_switches_to_dashboard() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)); // Navigate away first.
    assert_eq!(app.active_view, View::Projects);

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

// ── Navigation resets state ──────────────────────────────────────────────────

#[test]
fn test_view_switch_resets_input_mode() {
    let mut app = make_app();
    // First enter filter mode.
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(app.input_mode, InputMode::Filter);

    // Exit filter mode with Escape first.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.input_mode, InputMode::Normal);

    // Now switch view - input mode should stay Normal.
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    assert_eq!(
        app.input_mode,
        InputMode::Normal,
        "view switch should keep input mode as Normal"
    );
}

#[test]
fn test_view_switch_clears_help() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)); // Show help.
    assert!(app.show_help);

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)); // Switch view.
    assert!(!app.show_help, "view switch should clear the help overlay");
}

#[test]
fn test_view_switch_closes_modal() {
    let mut app = make_app();
    // Navigate away and back to dashboard (no modal by default).
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    // View switch should work correctly without any modal interference.
    assert_eq!(app.active_view, View::Dashboard);
}

// ── Cursor movement tests ────────────────────────────────────────────────────

#[test]
fn test_j_key_does_not_panic_on_empty_list() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let initial_selected = app.projects.selected;

    // Pressing 'j' should not panic even on empty list.
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    // On empty list, selection should stay at 0.
    assert_eq!(app.projects.selected, initial_selected);
}

#[test]
fn test_k_key_moves_selection_up() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.projects.selected = 2; // Set a non-zero selection.

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.projects.selected, 1, "'k' should move selection up");
}

#[test]
fn test_down_arrow_does_not_panic_on_empty_list() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let initial_selected = app.projects.selected;

    // Pressing Down should not panic even on empty list.
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

    // On empty list, selection should stay at 0.
    assert_eq!(app.projects.selected, initial_selected);
}

#[test]
fn test_up_arrow_moves_selection_up() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.projects.selected = 2;

    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(
        app.projects.selected, 1,
        "Up arrow should move selection up"
    );
}

#[test]
fn test_selection_clamps_at_zero() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.projects.selected = 0;

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(
        app.projects.selected, 0,
        "selection should not go below zero"
    );
}

#[test]
fn test_j_key_affects_current_view_only() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.projects.selected = 0;

    // Press 'j' in projects view (doesn't move on empty list but doesn't error).
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    // Switch to tasks.
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // J should affect tasks, not projects.
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    // On empty tasks list, selection stays at 0.
    assert_eq!(
        app.tasks.selected, 0,
        "'j' should affect current view's selection"
    );
    // Projects selection should be unchanged.
    assert_eq!(
        app.projects.selected, 0,
        "projects selection should be unchanged"
    );
}

// ── Tab bar rendering tests ─────────────────────────────────────────────────

#[test]
fn test_navigation_updates_tab_bar_highlight_dashboard() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[D]ashboard "),
        "dashboard tab should be highlighted"
    );
}

#[test]
fn test_navigation_updates_tab_bar_highlight_projects() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[P]rojects "),
        "projects tab should be highlighted"
    );
}

#[test]
fn test_navigation_updates_tab_bar_highlight_tasks() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[T]asks "),
        "tasks tab should be highlighted"
    );
}

#[test]
fn test_navigation_updates_tab_bar_highlight_todos() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[O]Todos"),
        "todos tab should be highlighted"
    );
}

#[test]
fn test_navigation_updates_tab_bar_highlight_tracker() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "T[R]acker   "),
        "tracker tab should be highlighted"
    );
}

#[test]
fn test_navigation_updates_tab_bar_highlight_inbox() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[I]nbox    "),
        "inbox tab should be highlighted"
    );
}

#[test]
fn test_navigation_updates_tab_bar_highlight_reminders() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[M]Reminders"),
        "reminders tab should be highlighted"
    );
}

// ── View-specific content rendering ─────────────────────────────────────────

#[test]
fn test_projects_view_shows_correct_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[P]rojects "),
        "projects view should show tab bar"
    );
}

#[test]
fn test_tasks_view_shows_correct_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[T]asks "),
        "tasks view should show tab bar"
    );
}

#[test]
fn test_todos_view_shows_correct_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[O]Todos"),
        "todos view should show tab bar"
    );
}

#[test]
fn test_tracker_view_shows_correct_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "T[R]acker   "),
        "tracker view should show tab bar"
    );
}

#[test]
fn test_inbox_view_shows_correct_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[I]nbox    "),
        "inbox view should show tab bar"
    );
}

#[test]
fn test_reminders_view_shows_correct_content() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "[M]Reminders"),
        "reminders view should show tab bar"
    );
}

// ── Unknown key handling ─────────────────────────────────────────────────────

#[test]
fn test_unknown_key_does_not_crash() {
    let mut app = make_app();
    // Various unknown keys should not panic or corrupt state.
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));

    // State should remain consistent.
    assert_eq!(
        app.input_mode,
        InputMode::Normal,
        "unknown keys should not change input mode"
    );
    assert!(!app.should_quit, "unknown keys should not trigger quit");
}
