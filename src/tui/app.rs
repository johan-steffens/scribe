// Rust guideline compliant 2026-02-21
//! Application state machine for the Scribe TUI.
//!
//! [`App`] is the single source of truth for all TUI state. Views are pure
//! rendering functions that accept `&App` — they never mutate state.
//!
//! Key-event dispatching lives in [`super::keys`] to keep this file concise.
//! Data-refresh helpers live in the `refresh` sub-module.
//!
//! # Error handling
//!
//! All fallible operations store their error message in [`App::last_error`]
//! instead of panicking. The status bar renders this field in red when set.
//!
//! # Examples
//!
//! ```no_run
//! # use std::sync::{Arc, Mutex};
//! # use scribe::db::open_in_memory;
//! use scribe::tui::app::App;
//!
//! let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
//! let mut app = App::new(conn);
//! assert!(!app.should_quit);
//! ```

#[path = "app/refresh.rs"]
mod refresh;

use std::sync::{Arc, Mutex};

use chrono::Duration;
use crossterm::event::KeyEvent;
use rusqlite::Connection;

use crate::domain::TimeEntry;
use crate::ops::TrackerOps;
use crate::tui::types::{
    CaptureViewState, EntryViewState, Modal, ProjectViewState, ReminderViewState, TaskViewState,
    TodoViewState, ViewState,
};

// Re-export types so downstream modules can `use crate::tui::app::{App, View, InputMode}`.
#[doc(inline)]
pub use crate::tui::types::{InputMode, View};

// ── App ────────────────────────────────────────────────────────────────────

/// Central application state for the TUI.
///
/// All data displayed in the TUI is loaded into this struct before rendering
/// begins. The draw functions are pure — they only read from `App`.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::db::open_in_memory;
/// use scribe::tui::app::App;
///
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let mut app = App::new(conn);
/// app.tick();
/// ```
#[derive(Debug)]
pub struct App {
    /// The currently active view.
    pub active_view: View,
    /// Set to `true` to exit the event loop.
    pub should_quit: bool,
    /// Last error message, displayed in the status bar in red when `Some`.
    pub last_error: Option<String>,
    /// Active timer entry and its elapsed duration, refreshed on each tick.
    pub active_timer: Option<(TimeEntry, Duration)>,
    /// Whether the help overlay is visible.
    pub show_help: bool,
    /// Current input mode (normal navigation or filter entry).
    pub input_mode: InputMode,
    /// Active modal overlay (form, confirm dialog, or none).
    pub modal: Modal,
    /// Per-view list state for projects.
    pub projects: ProjectViewState,
    /// Per-view list state for tasks.
    pub tasks: TaskViewState,
    /// Per-view list state for todos.
    pub todos: TodoViewState,
    /// Per-view list state for time entries (tracker).
    pub entries: EntryViewState,
    /// Per-view list state for capture items (inbox).
    pub captures: CaptureViewState,
    /// Per-view list state for reminders.
    pub reminders: ReminderViewState,
    /// Shared database connection used to refresh data.
    pub(super) db: Arc<Mutex<Connection>>,
}

impl App {
    /// Creates a new [`App`], loads initial data, and returns it.
    ///
    /// On construction, `active_view` is set to `Dashboard` and data is
    /// fetched via [`App::refresh`]. Any initial load error is stored in
    /// `last_error` rather than propagated.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::db::open_in_memory;
    /// use scribe::tui::app::App;
    ///
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let app = App::new(conn);
    /// assert_eq!(app.active_view, scribe::tui::app::View::Dashboard);
    /// ```
    #[must_use]
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        let mut app = Self {
            active_view: View::Dashboard,
            should_quit: false,
            last_error: None,
            active_timer: None,
            show_help: false,
            input_mode: InputMode::Normal,
            modal: Modal::None,
            projects: ViewState::new(),
            tasks: ViewState::new(),
            todos: ViewState::new(),
            entries: ViewState::new(),
            captures: ViewState::new(),
            reminders: ViewState::new(),
            db,
        };
        app.refresh();
        app
    }

    /// Reloads all view data from the database.
    ///
    /// Called at startup and after any mutating operation. Errors are stored
    /// in [`App::last_error`]; the app never panics on DB failure.
    pub fn refresh(&mut self) {
        refresh::refresh_projects(self);
        refresh::refresh_tasks(self);
        refresh::refresh_todos(self);
        refresh::refresh_entries(self);
        refresh::refresh_captures(self);
        refresh::refresh_reminders(self);
    }

    /// Refreshes the active timer status from the database.
    ///
    /// Called on every event-loop iteration. Errors are stored in
    /// [`App::last_error`] without panicking.
    pub fn tick(&mut self) {
        let tracker = TrackerOps::new(Arc::clone(&self.db));
        match tracker.timer_status() {
            Ok(status) => {
                self.active_timer = status;
            }
            Err(e) => {
                self.last_error = Some(format!("timer status error: {e}"));
            }
        }
    }

    /// Dispatches a key event to the appropriate handler.
    ///
    /// Active modals receive key events first. When no modal is open, the
    /// event is forwarded to the view-level key handler.
    /// Key dispatch logic lives in [`crate::tui::keys`].
    pub fn handle_key(&mut self, key: KeyEvent) {
        crate::tui::keys::handle_key(self, key);
    }

    /// Returns the number of filtered items in the currently active list view.
    ///
    /// Used by [`crate::tui::keys`] to clamp cursor positions.
    pub(super) fn filtered_len(&self) -> usize {
        match self.active_view {
            View::Projects => Self::filter_count(
                &self.projects.filter,
                self.projects.items.len(),
                self.projects
                    .items
                    .iter()
                    .map(|p| format!("{} {}", p.slug, p.name)),
            ),
            View::Tasks | View::Dashboard => Self::filter_count(
                &self.tasks.filter,
                self.tasks.items.len(),
                self.tasks.items.iter().map(|t| t.title.clone()),
            ),
            View::Todos => Self::filter_count(
                &self.todos.filter,
                self.todos.items.len(),
                self.todos.items.iter().map(|t| t.title.clone()),
            ),
            View::Tracker => self.entries.items.len(),
            View::Inbox => Self::filter_count(
                &self.captures.filter,
                self.captures.items.len(),
                self.captures.items.iter().map(|c| c.body.clone()),
            ),
            View::Reminders => Self::filter_count(
                &self.reminders.filter,
                self.reminders.items.len(),
                self.reminders
                    .items
                    .iter()
                    .map(|r| r.message.as_deref().unwrap_or("").to_owned()),
            ),
        }
    }

    /// Returns a mutable reference to the `selected` cursor for the active view.
    ///
    /// Used by the key handler to move the list cursor.
    pub(super) fn selected_mut(&mut self) -> &mut usize {
        match self.active_view {
            View::Projects => &mut self.projects.selected,
            View::Tasks | View::Dashboard => &mut self.tasks.selected,
            View::Todos => &mut self.todos.selected,
            View::Tracker => &mut self.entries.selected,
            View::Inbox => &mut self.captures.selected,
            View::Reminders => &mut self.reminders.selected,
        }
    }

    /// Counts items whose searchable text contains the filter string.
    fn filter_count(filter: &str, total: usize, texts: impl Iterator<Item = String>) -> usize {
        if filter.is_empty() {
            return total;
        }
        let f = filter.to_lowercase();
        texts.filter(|t| t.to_lowercase().contains(&f)).count()
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;
    use crate::db::open_in_memory;

    fn make_app() -> App {
        let conn = Arc::new(Mutex::new(open_in_memory().expect("in-memory db")));
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
}
