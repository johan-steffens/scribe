// Rust guideline compliant 2026-02-21
//! Unit tests for [`crate::store::reminder_store::SqliteReminders`].

use chrono::Utc;
use scribe::domain::{NewReminder, ProjectId, Reminders};
use scribe::testing::reminder_store;

use reminder_store::store as make_store;

fn new_reminder(slug: &str) -> NewReminder {
    NewReminder {
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        task_id: None,
        remind_at: Utc::now(),
        message: Some("Reminder message".to_owned()),
        persistent: false,
    }
}

#[test]
fn test_create_and_find() {
    let s = make_store();
    let r = s.create(new_reminder("r1")).expect("create");
    assert_eq!(r.slug, "r1");
    assert!(!r.fired);
}

#[test]
fn test_archive_and_restore() {
    let s = make_store();
    s.create(new_reminder("r2")).expect("create");
    s.archive("r2").expect("archive");
    let items = s.list(None, false).expect("list");
    assert!(!items.iter().any(|r| r.slug == "r2"));
    s.restore("r2").expect("restore");
    let items = s.list(None, false).expect("list");
    assert!(items.iter().any(|r| r.slug == "r2"));
}
