// Rust guideline compliant 2026-02-21
//! Key-event dispatching for the Scribe TUI.
//!
//! [`handle_key`] and [`handle_filter_key`] are extracted here to keep
//! `app.rs` under the 400-line limit. Both functions take `&mut App` and
//! mutate state in place.
//!
//! Internal to the `tui` module — not part of the public API.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::App;
use crate::tui::types::{InputMode, View};

/// Dispatches a normal-mode key event to the appropriate action.
///
/// Global keys are handled first; per-view list navigation follows.
pub(super) fn handle_key(app: &mut App, key: KeyEvent) {
    // Filter mode: route to the filter handler instead.
    if app.input_mode == InputMode::Filter {
        handle_filter_key(app, key);
        return;
    }

    match key.code {
        // ── global navigation ──────────────────────────────────────────────
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('d') => {
            switch_view(app, View::Dashboard);
        }
        KeyCode::Char('p') => {
            switch_view(app, View::Projects);
        }
        KeyCode::Char('t') => {
            switch_view(app, View::Tasks);
        }
        KeyCode::Char('o') => {
            switch_view(app, View::Todos);
        }
        KeyCode::Char('r') => {
            switch_view(app, View::Tracker);
        }
        KeyCode::Char('i') => {
            switch_view(app, View::Inbox);
        }
        KeyCode::Char('m') => {
            switch_view(app, View::Reminders);
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }
        // Ctrl-C as a universal quit fallback.
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        // ── per-view navigation ────────────────────────────────────────────
        KeyCode::Char('j') | KeyCode::Down => {
            move_selection_down(app);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            move_selection_up(app);
        }
        KeyCode::Enter => {
            // Detail view not yet implemented in Phase 3.
            app.last_error = Some("detail view not yet implemented (Phase 4)".to_owned());
        }
        KeyCode::Char('n' | 'e') => {
            // Create / edit not yet implemented in Phase 3.
            app.last_error = Some("create/edit not yet implemented (Phase 4)".to_owned());
        }
        KeyCode::Char('D') => {
            // Delete (shift-D) not yet implemented in Phase 3.
            app.last_error = Some("delete not yet implemented (Phase 4)".to_owned());
        }
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Filter;
            // Clear any previous filter when entering filter mode.
            current_filter_mut(app).clear();
        }
        KeyCode::Esc => {
            // Dismiss error message if one is displayed.
            if app.last_error.is_some() {
                app.last_error = None;
            }
            // Also hide help overlay.
            app.show_help = false;
        }
        _ => {}
    }
}

/// Handles a key event while the app is in filter input mode.
pub(super) fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            // Exit filter mode and clear the filter.
            app.input_mode = InputMode::Normal;
            current_filter_mut(app).clear();
            // Reset selection since the visible set may have changed.
            match app.active_view {
                View::Projects => app.projects.selected = 0,
                _ => app.tasks.selected = 0,
            }
        }
        KeyCode::Enter => {
            // Confirm filter and return to normal mode.
            app.input_mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            current_filter_mut(app).pop();
            // Clamp selection after filter narrows the list.
            let new_len = app.filtered_len();
            match app.active_view {
                View::Projects => {
                    if app.projects.selected >= new_len && new_len > 0 {
                        app.projects.selected = new_len - 1;
                    }
                }
                _ => {
                    if app.tasks.selected >= new_len && new_len > 0 {
                        app.tasks.selected = new_len - 1;
                    }
                }
            }
        }
        KeyCode::Char(c) => {
            current_filter_mut(app).push(c);
            // Reset selection to top when filter changes.
            match app.active_view {
                View::Projects => app.projects.selected = 0,
                _ => app.tasks.selected = 0,
            }
        }
        _ => {}
    }
}

// ── private helpers ────────────────────────────────────────────────────────

/// Switches to `view`, resetting mode, help, and filter.
fn switch_view(app: &mut App, view: View) {
    app.active_view = view;
    app.input_mode = InputMode::Normal;
    app.show_help = false;
    // Clear filter on view switch so navigation starts fresh.
    current_filter_mut(app).clear();
}

/// Moves the selection cursor down by one in the active list view.
fn move_selection_down(app: &mut App) {
    let filtered_len = app.filtered_len();
    if filtered_len == 0 {
        return;
    }
    match app.active_view {
        View::Projects => {
            if app.projects.selected + 1 < filtered_len {
                app.projects.selected += 1;
            }
        }
        _ => {
            if app.tasks.selected + 1 < filtered_len {
                app.tasks.selected += 1;
            }
        }
    }
}

/// Moves the selection cursor up by one in the active list view.
fn move_selection_up(app: &mut App) {
    match app.active_view {
        View::Projects => {
            if app.projects.selected > 0 {
                app.projects.selected -= 1;
            }
        }
        _ => {
            if app.tasks.selected > 0 {
                app.tasks.selected -= 1;
            }
        }
    }
}

/// Returns a mutable reference to the filter string for the active view.
fn current_filter_mut(app: &mut App) -> &mut String {
    match app.active_view {
        View::Projects => &mut app.projects.filter,
        // Tasks, Dashboard, and all Phase-4 views share the tasks filter.
        _ => &mut app.tasks.filter,
    }
}
