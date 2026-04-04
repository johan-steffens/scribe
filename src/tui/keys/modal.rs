//! Modal key handlers for the Scribe TUI.
//!
//! Handles key events when a [`Modal`] (confirm dialog or form) is active.
//! All mutations go through the `actions` sub-module after a form is submitted.

use crossterm::event::KeyEvent;

use crate::tui::app::App;
use crate::tui::components::dialog::{ConfirmDialog, DialogResponse};
use crate::tui::components::form::FormState;
use crate::tui::types::Modal;

use super::actions;

/// Routes a key event to the active modal.
pub(super) fn handle_modal_key(app: &mut App, key: KeyEvent) {
    match &app.modal {
        Modal::None => {}
        Modal::Confirm(_, _) => handle_confirm_key(app, key.code),
        Modal::Form(_, _) => handle_form_key(app, key),
    }
}

/// Handles a key while a confirmation dialog is active.
fn handle_confirm_key(app: &mut App, code: crossterm::event::KeyCode) {
    let Modal::Confirm(_, ref ctx) = app.modal else {
        return;
    };

    let response = ConfirmDialog::handle_key(code);
    let ctx = ctx.clone();

    match response {
        DialogResponse::Confirmed => {
            app.modal = Modal::None;
            actions::execute_confirm(app, &ctx);
        }
        DialogResponse::Cancelled => {
            app.modal = Modal::None;
        }
        DialogResponse::Pending => {}
    }
}

/// Handles a key while a form modal is active.
fn handle_form_key(app: &mut App, key: KeyEvent) {
    let Modal::Form(ref mut form, _) = app.modal else {
        return;
    };
    form.handle_key(key.code, key.modifiers);

    match form.state() {
        FormState::Cancelled => {
            app.modal = Modal::None;
        }
        FormState::Submitted => {
            let Modal::Form(form, ctx) = std::mem::replace(&mut app.modal, Modal::None) else {
                return;
            };
            actions::execute_form(app, &form, ctx);
        }
        FormState::Open => {}
    }
}
