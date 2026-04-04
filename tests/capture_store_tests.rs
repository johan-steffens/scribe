// Rust guideline compliant 2026-02-21
//! Unit tests for [`crate::store::capture_store::SqliteCaptureItems`].

use chrono::Utc;
use scribe::domain::{CaptureItems, NewCaptureItem};
use scribe::testing::capture_store;

use capture_store::store as make_store;

fn new_item(slug: &str, body: &str) -> NewCaptureItem {
    NewCaptureItem {
        slug: slug.to_owned(),
        body: body.to_owned(),
        created_at: Utc::now(),
    }
}

#[test]
fn test_create_and_find() {
    let s = make_store();
    let item = s
        .create(new_item("c1", "Remember to buy milk"))
        .expect("create");
    assert_eq!(item.slug, "c1");
    assert!(!item.processed);
}

#[test]
fn test_mark_processed() {
    let s = make_store();
    s.create(new_item("c2", "Call dentist")).expect("create");
    let updated = s.mark_processed("c2").expect("mark");
    assert!(updated.processed);
}

#[test]
fn test_list_excludes_processed() {
    let s = make_store();
    s.create(new_item("c3", "Unprocessed")).expect("c3");
    s.create(new_item("c4", "Processed")).expect("c4");
    s.mark_processed("c4").expect("mark");
    let items = s.list(false).expect("list");
    assert!(items.iter().any(|i| i.slug == "c3"));
    assert!(!items.iter().any(|i| i.slug == "c4"));
}
