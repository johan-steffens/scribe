// Rust guideline compliant 2026-02-21
//! Integration tests for the dashboard view using `ratatui`'s `TestBackend`.
//!
//! These tests render the dashboard into a virtual terminal buffer and assert
//! on the visual output — verifying that the correct content appears at the
//! expected screen positions.

use std::sync::{Arc, Mutex};

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;
use scribe::db;
use scribe::tui::app::App;
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

// ── panel title tests ─────────────────────────────────────────────────────────

#[test]
fn test_dashboard_renders_today_tasks_panel_title() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, " Today's Tasks "),
        "buffer should contain \" Today's Tasks \" panel title"
    );
}

#[test]
fn test_dashboard_renders_active_timer_panel_title() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, " Active Timer "),
        "buffer should contain \" Active Timer \" panel title"
    );
}

// ── empty state tests ─────────────────────────────────────────────────────────

#[test]
fn test_dashboard_shows_no_tasks_message_when_empty() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    // With no tasks due today, the empty-state message should be shown.
    assert!(
        buffer_contains(&buf, " No tasks due today."),
        "buffer should contain \" No tasks due today.\" when no tasks are due"
    );
}

#[test]
fn test_dashboard_shows_no_timer_message_when_inactive() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    // No active timer should show the placeholder message.
    assert!(
        buffer_contains(&buf, " No active timer"),
        "buffer should contain \" No active timer\" when no timer is running"
    );
}

// ── tab bar tests ─────────────────────────────────────────────────────────────

#[test]
fn test_dashboard_renders_scribe_branding_in_tab_bar() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    assert!(
        buffer_contains(&buf, "  Scribe  "),
        "buffer should contain \"  Scribe  \" branding in tab bar"
    );
}

#[test]
fn test_dashboard_renders_navigation_keys_in_tab_bar() {
    let app = make_app();
    let buf = render_to_buffer(&app, 80, 24);

    // The tab bar shows navigation shortcuts.
    assert!(
        buffer_contains(&buf, "[D]ashboard "),
        "buffer should contain [D]ashboard shortcut"
    );
    assert!(
        buffer_contains(&buf, "[P]rojects "),
        "buffer should contain [P]rojects shortcut"
    );
    assert!(
        buffer_contains(&buf, "[T]asks "),
        "buffer should contain [T]asks shortcut"
    );
}

// ── viewport boundary tests ───────────────────────────────────────────────────

#[test]
fn test_dashboard_respects_small_terminal_dimensions() {
    let app = make_app();

    // Very small but valid terminal.
    let buf = render_to_buffer(&app, 20, 10);

    // Should still render something — the layout must not panic.
    assert!(!buf.area().is_empty(), "buffer should have a non-zero area");
}

#[test]
fn test_dashboard_buffer_has_correct_area_dimensions() {
    let app = make_app();
    let width = 80u16;
    let height = 24u16;
    let buf = render_to_buffer(&app, width, height);

    assert_eq!(
        buf.area().width,
        width,
        "buffer width should match terminal width"
    );
    assert_eq!(
        buf.area().height,
        height,
        "buffer height should match terminal height"
    );
}
