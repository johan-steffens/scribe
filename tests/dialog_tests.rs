// Rust guideline compliant 2026-02-21
//! Unit tests for the dialog component.

use crossterm::event::KeyCode;
use scribe::tui::components::dialog::{ConfirmDialog, DialogResponse};

#[test]
fn test_confirm_on_y() {
    assert_eq!(
        ConfirmDialog::handle_key(KeyCode::Char('y')),
        DialogResponse::Confirmed
    );
}

#[test]
fn test_confirm_on_enter() {
    assert_eq!(
        ConfirmDialog::handle_key(KeyCode::Enter),
        DialogResponse::Confirmed
    );
}

#[test]
fn test_cancel_on_n() {
    assert_eq!(
        ConfirmDialog::handle_key(KeyCode::Char('n')),
        DialogResponse::Cancelled
    );
}

#[test]
fn test_cancel_on_esc() {
    assert_eq!(
        ConfirmDialog::handle_key(KeyCode::Esc),
        DialogResponse::Cancelled
    );
}

#[test]
fn test_other_key_pending() {
    assert_eq!(
        ConfirmDialog::handle_key(KeyCode::Char('x')),
        DialogResponse::Pending
    );
}

#[test]
fn test_message_accessor() {
    let d = ConfirmDialog::new("Are you sure?");
    assert_eq!(d.message(), "Are you sure?");
}
