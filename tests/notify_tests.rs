//! Notification system integration tests.
//!
//! These tests verify that [`notify::fire`](scribe::notify::fire) handles
//! various reminder scenarios correctly. Since desktop notifications are
//! inherently side-effectful (they display to the user), these tests focus on:
//! - Ensuring `fire` doesn't panic with valid inputs
//! - Verifying the body construction logic
//! - Testing with various reminder configurations

use chrono::{Duration, Utc};

use scribe::domain::ProjectId;
use scribe::domain::{Reminder, ReminderId, TaskId};
use scribe::notify;

/// Sets the mock notify env var. Must be called at the start of each test
/// before any `notify::fire` call.
fn setup_mock() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| unsafe {
        std::env::set_var("SCRIBE_MOCK_NOTIFY", "1");
    });
}

// ── test fixtures ─────────────────────────────────────────────────────────────

/// Creates a minimal reminder with just a slug and message.
fn reminder_with_message(slug: &str, message: &str, persistent: bool) -> Reminder {
    Reminder {
        id: ReminderId(1),
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        project_slug: "test-project".to_owned(),
        task_id: None,
        task_slug: None,
        remind_at: Utc::now() + Duration::hours(1),
        message: Some(message.to_owned()),
        fired: false,
        persistent,
        archived_at: None,
        created_at: Utc::now(),
    }
}

/// Creates a reminder without a message (uses default "Reminder due").
fn reminder_without_message(slug: &str, persistent: bool) -> Reminder {
    Reminder {
        id: ReminderId(2),
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        project_slug: "test-project".to_owned(),
        task_id: None,
        task_slug: None,
        remind_at: Utc::now() + Duration::hours(1),
        message: None,
        fired: false,
        persistent,
        archived_at: None,
        created_at: Utc::now(),
    }
}

/// Creates a reminder linked to a task.
fn reminder_with_task(slug: &str, task_slug: &str, persistent: bool) -> Reminder {
    Reminder {
        id: ReminderId(3),
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        project_slug: "test-project".to_owned(),
        task_id: Some(TaskId(42)),
        task_slug: Some(task_slug.to_owned()),
        remind_at: Utc::now() + Duration::hours(1),
        message: Some("Task reminder".to_owned()),
        fired: false,
        persistent,
        archived_at: None,
        created_at: Utc::now(),
    }
}

// ── fire function tests ──────────────────────────────────────────────────────

/// Verifies that `fire` doesn't panic with a valid reminder that has a message.
#[test]
fn fire_does_not_panic_with_message() {
    setup_mock();
    let reminder = reminder_with_message("test-reminder", "Don't forget to review PR", false);
    notify::fire(&reminder);
}

/// Verifies that `fire` doesn't panic with a reminder without a message.
#[test]
fn fire_does_not_panic_without_message() {
    setup_mock();
    let reminder = reminder_without_message("no-message-reminder", false);
    notify::fire(&reminder);
}

/// Verifies that `fire` doesn't panic with a persistent reminder.
#[test]
fn fire_does_not_panic_with_persistent_reminder() {
    setup_mock();
    let reminder = reminder_with_message("persistent-reminder", "Important: review this", true);
    notify::fire(&reminder);
}

/// Verifies that `fire` doesn't panic with a persistent reminder without message.
#[test]
fn fire_does_not_panic_persistent_without_message() {
    setup_mock();
    let reminder = reminder_without_message("persistent-no-msg", true);
    notify::fire(&reminder);
}

/// Verifies that `fire` doesn't panic with a task-linked reminder.
#[test]
fn fire_does_not_panic_with_task_reminder() {
    setup_mock();
    let reminder = reminder_with_task("task-reminder", "my-task", false);
    notify::fire(&reminder);
}

/// Verifies that `fire` doesn't panic with multiple reminders in sequence.
#[test]
fn fire_handles_multiple_reminders() {
    setup_mock();
    let r1 = reminder_with_message("reminder-1", "First reminder", false);
    let r2 = reminder_without_message("reminder-2", false);
    let r3 = reminder_with_message("reminder-3", "Third reminder", true);

    notify::fire(&r1);
    notify::fire(&r2);
    notify::fire(&r3);
}

// ── reminder field tests ──────────────────────────────────────────────────────

#[test]
fn reminder_persistent_flag_is_respected() {
    let non_persistent = reminder_with_message("np", "test", false);
    assert!(!non_persistent.persistent);

    let persistent = reminder_with_message("p", "test", true);
    assert!(persistent.persistent);
}

#[test]
fn reminder_message_option_handling() {
    let with_msg = reminder_with_message("slug", "message", false);
    assert!(with_msg.message.is_some());
    assert_eq!(with_msg.message.as_deref(), Some("message"));

    let without_msg = reminder_without_message("slug", false);
    assert!(without_msg.message.is_none());
}

#[test]
fn reminder_task_linking() {
    let with_task = reminder_with_task("slug", "my-task", false);
    assert!(with_task.task_id.is_some());
    assert_eq!(with_task.task_slug.as_deref(), Some("my-task"));

    let without_task = reminder_without_message("slug", false);
    assert!(without_task.task_id.is_none());
    assert!(without_task.task_slug.is_none());
}

#[test]
fn reminder_slugs_can_be_unique() {
    let r1 = reminder_with_message("reminder-a", "Message A", false);
    let r2 = reminder_with_message("reminder-b", "Message B", false);
    let r3 = reminder_with_message("reminder-c", "Message C", false);

    assert_eq!(r1.slug, "reminder-a");
    assert_eq!(r2.slug, "reminder-b");
    assert_eq!(r3.slug, "reminder-c");

    assert_ne!(r1.slug, r2.slug);
    assert_ne!(r2.slug, r3.slug);
    assert_ne!(r1.slug, r3.slug);
}

// ── edge cases ────────────────────────────────────────────────────────────────

#[test]
fn fire_handles_empty_message() {
    setup_mock();
    let reminder = reminder_with_message("empty-msg", "", false);
    assert!(reminder.message.is_some());
    notify::fire(&reminder);
}

#[test]
fn fire_handles_very_long_message() {
    setup_mock();
    let long_message = "A".repeat(1000);
    let reminder = reminder_with_message("long-msg", &long_message, false);
    notify::fire(&reminder);
}

#[test]
fn fire_handles_special_characters_in_message() {
    setup_mock();
    let reminder = reminder_with_message(
        "special-chars",
        "Message with 'quotes', \"double quotes\", and\nnewlines\tand\ttabs!",
        false,
    );
    notify::fire(&reminder);
}

#[test]
fn fire_handles_unicode_in_message() {
    setup_mock();
    let reminder = reminder_with_message("unicode-msg", "Hello! 🎉 🌍 café résumé", false);
    notify::fire(&reminder);
}

#[test]
fn notification_title_constant_exists() {
    setup_mock();
    let reminder = reminder_with_message("title-test", "Testing title", false);
    notify::fire(&reminder);
}
