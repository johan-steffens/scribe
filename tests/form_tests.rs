// Rust guideline compliant 2026-02-21
//! Unit tests for the form component.

use crossterm::event::{KeyCode, KeyModifiers};
use scribe::tui::components::form::{Form, FormField, FormState};

fn text_field(label: &str, value: &str) -> FormField {
    FormField::Text {
        label: label.into(),
        value: value.into(),
        placeholder: String::new(),
        cursor: value.len(),
    }
}

fn make_form(fields: Vec<FormField>) -> Form {
    Form::new("Test", fields)
}

#[test]
fn test_form_starts_open() {
    let form = make_form(vec![text_field("Name", "")]);
    assert_eq!(form.state(), FormState::Open);
}

#[test]
fn test_esc_cancels() {
    let mut form = make_form(vec![text_field("Name", "")]);
    form.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(form.state(), FormState::Cancelled);
}

#[test]
fn test_enter_on_last_field_submits() {
    let mut form = make_form(vec![text_field("Name", "hello")]);
    form.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(form.state(), FormState::Submitted);
}

#[test]
fn test_text_input_appends() {
    let mut form = make_form(vec![text_field("Title", "")]);
    form.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(form.field_value(0), "a");
}

#[test]
fn test_text_backspace() {
    let mut form = make_form(vec![text_field("Title", "ab")]);
    form.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(form.field_value(0), "a");
}

#[test]
fn test_select_navigate() {
    let mut form = make_form(vec![FormField::Select {
        label: "Project".into(),
        options: vec!["alpha".into(), "beta".into()],
        selected: 0,
    }]);
    form.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
    assert_eq!(form.select_index(0), 1);
    form.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
    assert_eq!(form.select_index(0), 0);
}
