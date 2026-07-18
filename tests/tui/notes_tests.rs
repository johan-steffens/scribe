//! TUI notes view smoke tests (`TestBackend`).

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use scribe::domain::{NewNote, Notes};
use scribe::store::SqliteNotes;
use scribe::tui::app::{App, View};
use scribe::tui::ui;

fn make_app() -> App {
    let conn = Arc::new(Mutex::new(
        scribe::db::open_in_memory().expect("in-memory db"),
    ));
    App::new(conn, None)
}

fn render_to_buffer(app: &App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| ui::draw(frame, app)).expect("draw");
    terminal.backend().buffer().clone()
}

fn buffer_contains(buf: &Buffer, needle: &str) -> bool {
    let area = buf.area();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
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

#[test]
fn test_n_switches_to_notes_view() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Notes);
}

#[test]
fn test_notes_view_renders_seeded_note() {
    let conn = Arc::new(Mutex::new(
        scribe::db::open_in_memory().expect("in-memory db"),
    ));
    SqliteNotes::new(Arc::clone(&conn))
        .create(NewNote {
            slug: "arch-draft".into(),
            title: "Architecture Draft".into(),
            content: "Hello **markdown** and [[some-task]].".into(),
        })
        .expect("seed note");

    let mut app = App::new(conn, None);
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(app.active_view, View::Notes);

    let buf = render_to_buffer(&app, 120, 40);
    assert!(
        buffer_contains(&buf, "Architecture Draft") || buffer_contains(&buf, "arch-draft"),
        "notes list should show the seeded note"
    );
    assert!(
        buffer_contains(&buf, "Notes") || buffer_contains(&buf, "[N]Notes"),
        "tab bar should highlight Notes"
    );
}

#[test]
fn test_shift_n_opens_new_note_form() {
    let mut app = make_app();
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE));
    // Modal is crate-private; assert via Debug + rendered chrome.
    let modal = format!("{:?}", app.modal);
    assert!(
        modal.starts_with("Form"),
        "Shift+N in Notes view must open create form, got {modal}"
    );
    let buf = render_to_buffer(&app, 100, 30);
    assert!(
        buffer_contains(&buf, "New Note") || buffer_contains(&buf, "Title"),
        "create-note form should be visible"
    );
}
