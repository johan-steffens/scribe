//! Unit tests for [`crate::tui::app::App`].

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use scribe::db;
use scribe::tui::app::{App, InputMode, View};

fn make_app() -> App {
    let conn = Arc::new(Mutex::new(db::open_in_memory().expect("in-memory db")));
    App::new(conn)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn test_new_app_starts_on_dashboard() {
    let app = make_app();
    assert_eq!(app.active_view, View::Dashboard);
    assert!(!app.should_quit);
}

#[test]
fn test_quit_key_sets_should_quit() {
    let mut app = make_app();
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_view_switch_keys() {
    let mut app = make_app();
    app.handle_key(key(KeyCode::Char('p')));
    assert_eq!(app.active_view, View::Projects);
    app.handle_key(key(KeyCode::Char('t')));
    assert_eq!(app.active_view, View::Tasks);
    app.handle_key(key(KeyCode::Char('d')));
    assert_eq!(app.active_view, View::Dashboard);
    app.handle_key(key(KeyCode::Char('o')));
    assert_eq!(app.active_view, View::Todos);
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.active_view, View::Tracker);
    app.handle_key(key(KeyCode::Char('i')));
    assert_eq!(app.active_view, View::Inbox);
    app.handle_key(key(KeyCode::Char('m')));
    assert_eq!(app.active_view, View::Reminders);
}

#[test]
fn test_help_overlay_toggle() {
    let mut app = make_app();
    assert!(!app.show_help);
    app.handle_key(key(KeyCode::Char('?')));
    assert!(app.show_help);
    app.handle_key(key(KeyCode::Char('?')));
    assert!(!app.show_help);
}

#[test]
fn test_filter_mode_entry_and_exit() {
    let mut app = make_app();
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('/')));
    assert_eq!(app.input_mode, InputMode::Filter);
    app.handle_key(key(KeyCode::Char('a')));
    assert_eq!(app.projects.filter, "a");
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.input_mode, InputMode::Normal);
    assert!(app.projects.filter.is_empty());
}

#[test]
fn test_refresh_loads_data() {
    let app = make_app();
    // quick-capture project should always be present.
    assert!(!app.projects.items.is_empty());
}
