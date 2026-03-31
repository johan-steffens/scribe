// Rust guideline compliant 2026-02-21
//! Unit tests for [`SqliteProjects`].
//!
//! This file is included by `project_store.rs` via `#[path = "project_store_tests.rs"]`.

use super::*;
use crate::db::open_in_memory;

fn store() -> SqliteProjects {
    let conn = open_in_memory().expect("in-memory db");
    SqliteProjects::new(Arc::new(Mutex::new(conn)))
}

fn new_project(slug: &str, name: &str) -> NewProject {
    NewProject {
        slug: slug.to_owned(),
        name: name.to_owned(),
        description: None,
        status: ProjectStatus::Active,
    }
}

#[test]
fn test_create_and_find_by_slug() {
    let s = store();
    let p = s.create(new_project("alpha", "Alpha")).expect("create");
    assert_eq!(p.slug, "alpha");
    let found = s.find_by_slug("alpha").expect("find").expect("some");
    assert_eq!(found.id, p.id);
}

#[test]
fn test_list_active_excludes_archived() {
    let s = store();
    s.create(new_project("p1", "P1")).expect("p1");
    s.create(new_project("p2", "P2")).expect("p2");
    s.archive("p1").expect("archive");
    let active = s.list_active().expect("list");
    assert!(active.iter().any(|p| p.slug == "p2"));
    assert!(!active.iter().any(|p| p.slug == "p1"));
}

#[test]
fn test_archive_blocked_on_reserved() {
    let s = store();
    let err = s.archive("quick-capture").unwrap_err();
    assert!(err.to_string().contains("reserved"));
}

#[test]
fn test_delete_blocked_on_reserved() {
    let s = store();
    let err = s.delete("quick-capture").unwrap_err();
    assert!(err.to_string().contains("reserved"));
}

#[test]
fn test_restore_clears_archived_at() {
    let s = store();
    s.create(new_project("r1", "R1")).expect("create");
    s.archive("r1").expect("archive");
    let restored = s.restore("r1").expect("restore");
    assert!(restored.archived_at.is_none());
}

#[test]
fn test_update_name() {
    let s = store();
    s.create(new_project("upd", "Old Name")).expect("create");
    let updated = s
        .update(
            "upd",
            ProjectPatch {
                name: Some("New Name".to_owned()),
                ..Default::default()
            },
        )
        .expect("update");
    assert_eq!(updated.name, "New Name");
}

#[test]
fn test_list_filtered_by_status() {
    let s = store();
    s.create(NewProject {
        status: ProjectStatus::Paused,
        ..new_project("paused-one", "Paused One")
    })
    .expect("create paused");
    let paused = s
        .list(Some(ProjectStatus::Paused), false)
        .expect("list paused");
    assert!(paused.iter().all(|p| p.status == ProjectStatus::Paused));
    assert!(paused.iter().any(|p| p.slug == "paused-one"));
}
