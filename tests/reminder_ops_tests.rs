// Rust guideline compliant 2026-02-21
//! Unit tests for [`crate::ops::reminders::ReminderOps`].

use scribe::domain::ReminderPatch;
use scribe::ops::reminders::CreateReminder;
use scribe::testing::reminder_ops;

use reminder_ops::future;
use reminder_ops::ops as make_ops;

#[test]
fn test_create_reminder() {
    let ops = make_ops();
    let r = ops
        .create(CreateReminder {
            project_slug: "quick-capture".to_owned(),
            task_slug: None,
            remind_at: future(),
            message: Some("Deploy on Friday".to_owned()),
            persistent: false,
        })
        .expect("create");
    assert!(r.slug.starts_with("quick-capture-reminder-"));
    assert!(!r.fired);
}

#[test]
fn test_create_project_not_found_returns_error() {
    let ops = make_ops();
    let err = ops
        .create(CreateReminder {
            project_slug: "nonexistent".to_owned(),
            task_slug: None,
            remind_at: future(),
            message: None,
            persistent: false,
        })
        .unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_delete_requires_archived() {
    let ops = make_ops();
    let r = ops
        .create(CreateReminder {
            project_slug: "quick-capture".to_owned(),
            task_slug: None,
            remind_at: future(),
            message: Some("Active".to_owned()),
            persistent: false,
        })
        .expect("create");
    let err = ops.delete(&r.slug).unwrap_err();
    assert!(err.to_string().contains("archived"));
}

#[test]
fn test_update_changes_message() {
    let ops = make_ops();
    let r = ops
        .create(CreateReminder {
            project_slug: "quick-capture".to_owned(),
            task_slug: None,
            remind_at: future(),
            message: Some("Original".to_owned()),
            persistent: false,
        })
        .expect("create");
    let updated = ops
        .update(
            &r.slug,
            ReminderPatch {
                remind_at: None,
                message: Some("Updated".to_owned()),
                persistent: None,
            },
        )
        .expect("update");
    assert_eq!(updated.message.as_deref(), Some("Updated"));
}
