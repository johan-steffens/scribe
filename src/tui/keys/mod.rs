//! Key-event dispatching for the Scribe TUI.
//!
//! [`handle_key`] is the single entry point called by [`App::handle_key`].
//! It routes events through three layers in order:
//!
//! 1. **Modal** — if a [`Modal`] is active, all keys go to the modal handler.
//! 2. **Filter mode** — if `input_mode == Filter`, keys update the filter string.
//! 3. **Normal mode** — global and per-view key bindings.
//!
//! # Sub-modules
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`modal`] | Confirm/form modal key handlers |
//! | [`actions`] | Ops-layer mutations called after form submission |
//! | [`forms`] | Form and field builders for create/edit operations |
//! | [`view_handlers`] | Per-view `n`/`e`/`D`/`Space`/`Enter` handlers |
//! | [`helpers`] | Selection, navigation, and utility functions |
//!
//! Internal to the `tui` module — not part of the public API.

pub(super) mod actions;
pub(super) mod forms;
pub(super) mod helpers;
pub(super) mod modal;
pub(super) mod view_handlers;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::App;
use crate::tui::types::{InputMode, Modal, View};

// ── main entry point ───────────────────────────────────────────────────────

/// Dispatches a normal-mode key event to the appropriate action.
///
/// Routing order: active modal → filter mode → normal/per-view.
pub(super) fn handle_key(app: &mut App, key: KeyEvent) {
    // 1. Modal takes all keys first.
    if !matches!(app.modal, Modal::None) {
        modal::handle_modal_key(app, key);
        return;
    }

    // 2. Filter mode.
    if app.input_mode == InputMode::Filter {
        handle_filter_key(app, key);
        return;
    }

    // 3. Normal mode.
    handle_normal_key(app, key);
}

// ── normal key handler ─────────────────────────────────────────────────────

/// Handles a key event in normal navigation mode.
fn handle_normal_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // ── global navigation ──────────────────────────────────────────────
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('d') => helpers::switch_view(app, View::Dashboard),
        KeyCode::Char('p') => helpers::switch_view(app, View::Projects),
        KeyCode::Char('t') => helpers::switch_view(app, View::Tasks),
        KeyCode::Char('o') => helpers::switch_view(app, View::Todos),
        KeyCode::Char('r') => helpers::switch_view(app, View::Tracker),
        KeyCode::Char('i') => helpers::switch_view(app, View::Inbox),
        KeyCode::Char('m') => helpers::switch_view(app, View::Reminders),
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }
        // Ctrl-C as a universal quit fallback.
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        // ── per-view navigation ────────────────────────────────────────────
        KeyCode::Char('j') | KeyCode::Down => helpers::move_selection_down(app),
        KeyCode::Char('k') | KeyCode::Up => helpers::move_selection_up(app),
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Filter;
            helpers::current_filter_mut(app).clear();
        }
        KeyCode::Esc => {
            if app.last_error.is_some() {
                app.last_error = None;
            }
            app.show_help = false;
        }
        // ── create / edit / delete ─────────────────────────────────────────
        KeyCode::Char('n') => view_handlers::handle_new(app),
        KeyCode::Char('e') => view_handlers::handle_edit(app),
        KeyCode::Char('D') => view_handlers::handle_delete(app),
        KeyCode::Char(' ') => view_handlers::handle_space(app),
        KeyCode::Enter => view_handlers::handle_enter(app),
        // ── todo-specific move ─────────────────────────────────────────────
        KeyCode::Char('v') if app.active_view == View::Todos => {
            view_handlers::handle_move_todo(app);
        }
        _ => {}
    }
}

// ── filter key handler ─────────────────────────────────────────────────────

/// Handles a key event while the app is in filter input mode.
pub(super) fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            helpers::current_filter_mut(app).clear();
            *app.selected_mut() = 0;
        }
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            helpers::current_filter_mut(app).pop();
            let new_len = app.filtered_len();
            let sel = app.selected_mut();
            if *sel >= new_len && new_len > 0 {
                *sel = new_len - 1;
            }
        }
        KeyCode::Char(c) => {
            helpers::current_filter_mut(app).push(c);
            *app.selected_mut() = 0;
        }
        _ => {}
    }
}
