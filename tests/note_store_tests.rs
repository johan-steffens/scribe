//! Unit tests for [`crate::store::note_store::SqliteNotes`] and [`crate::store::note_store::SqliteLinks`].

use scribe::domain::{Links, NewNote, NotePatch, Notes};
use scribe::testing::note_store;

use note_store::links_store as make_links;
use note_store::notes_store as make_notes;

fn new_note(slug: &str, title: &str, content: &str) -> NewNote {
    NewNote {
        slug: slug.to_owned(),
        title: title.to_owned(),
        content: content.to_owned(),
    }
}

// ── SqliteNotes tests ─────────────────────────────────────────────────────────

#[test]
fn test_create_and_find() {
    let s = make_notes();
    let n = s
        .create(new_note(
            "arch-draft-2026",
            "Architecture Draft",
            "# Architecture\n\nDraft notes.",
        ))
        .expect("create");
    assert_eq!(n.slug, "arch-draft-2026");
    let found = s
        .find_by_slug("arch-draft-2026")
        .expect("find")
        .expect("some");
    assert_eq!(found.id, n.id);
    assert_eq!(found.title, "Architecture Draft");
}

#[test]
fn test_find_by_slug_not_found() {
    let s = make_notes();
    let result = s.find_by_slug("nonexistent").expect("find");
    assert!(result.is_none());
}

#[test]
fn test_list() {
    let s = make_notes();
    s.create(new_note("note-a", "Note A", "Content A"))
        .expect("create a");
    s.create(new_note("note-b", "Note B", "Content B"))
        .expect("create b");
    let all = s.list().expect("list");
    assert_eq!(all.len(), 2);
    // Ordered by created_at.
    assert_eq!(all[0].slug, "note-a");
    assert_eq!(all[1].slug, "note-b");
}

#[test]
fn test_update_title() {
    let s = make_notes();
    s.create(new_note("upd-title", "Original Title", "Body"))
        .expect("create");
    let updated = s
        .update(
            "upd-title",
            NotePatch {
                title: Some("Updated Title".to_owned()),
                ..Default::default()
            },
        )
        .expect("update");
    assert_eq!(updated.title, "Updated Title");
    assert_eq!(updated.content, "Body"); // content unchanged
}

#[test]
fn test_update_content() {
    let s = make_notes();
    s.create(new_note("upd-content", "Title", "Original body"))
        .expect("create");
    let updated = s
        .update(
            "upd-content",
            NotePatch {
                content: Some("Updated body with more content.".to_owned()),
                ..Default::default()
            },
        )
        .expect("update");
    assert_eq!(updated.title, "Title");
    assert_eq!(updated.content, "Updated body with more content.");
}

#[test]
fn test_update_both_fields() {
    let s = make_notes();
    s.create(new_note("upd-both", "Original", "Original body"))
        .expect("create");
    let updated = s
        .update(
            "upd-both",
            NotePatch {
                title: Some("New Title".to_owned()),
                content: Some("New body".to_owned()),
            },
        )
        .expect("update");
    assert_eq!(updated.title, "New Title");
    assert_eq!(updated.content, "New body");
}

#[test]
fn test_update_not_found() {
    let s = make_notes();
    let err = s
        .update(
            "missing",
            NotePatch {
                title: Some("Title".to_owned()),
                ..Default::default()
            },
        )
        .expect_err("update should fail");
    assert!(err.to_string().contains("missing"));
}

#[test]
fn test_delete() {
    let s = make_notes();
    s.create(new_note("to-delete", "Delete Me", "Content"))
        .expect("create");
    s.delete("to-delete").expect("delete");
    assert!(s.find_by_slug("to-delete").expect("find").is_none());
}

#[test]
fn test_delete_not_found() {
    let s = make_notes();
    let err = s.delete("nonexistent").expect_err("delete should fail");
    assert!(err.to_string().contains("nonexistent"));
}

// ── SqliteLinks tests ────────────────────────────────────────────────────────

#[test]
fn test_create_and_find_outbound() {
    let s = make_links();
    let link = s.create("note-a", "note-b").expect("create link");
    assert_eq!(link.source_slug, "note-a");
    assert_eq!(link.target_slug, "note-b");

    let outbound = s.outbound_for("note-a").expect("outbound");
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0].target_slug, "note-b");
}

#[test]
fn test_create_and_find_inbound() {
    let s = make_links();
    s.create("note-a", "note-b").expect("create link");

    let inbound = s.inbound_for("note-b").expect("inbound");
    assert_eq!(inbound.len(), 1);
    assert_eq!(inbound[0].source_slug, "note-a");
}

#[test]
fn test_inbound_for_no_links() {
    let s = make_links();
    let inbound = s.inbound_for("orphan").expect("inbound");
    assert!(inbound.is_empty());
}

#[test]
fn test_outbound_for_no_links() {
    let s = make_links();
    let outbound = s.outbound_for("orphan").expect("outbound");
    assert!(outbound.is_empty());
}

#[test]
fn test_multiple_outbound_links() {
    let s = make_links();
    s.create("note-x", "note-a").expect("link 1");
    s.create("note-x", "note-b").expect("link 2");
    s.create("note-x", "note-c").expect("link 3");

    let outbound = s.outbound_for("note-x").expect("outbound");
    assert_eq!(outbound.len(), 3);
    let targets: Vec<_> = outbound.iter().map(|l| l.target_slug.as_str()).collect();
    assert!(targets.contains(&"note-a"));
    assert!(targets.contains(&"note-b"));
    assert!(targets.contains(&"note-c"));
}

#[test]
fn test_multiple_inbound_links() {
    let s = make_links();
    s.create("note-a", "shared-target").expect("link 1");
    s.create("note-b", "shared-target").expect("link 2");

    let inbound = s.inbound_for("shared-target").expect("inbound");
    assert_eq!(inbound.len(), 2);
    let sources: Vec<_> = inbound.iter().map(|l| l.source_slug.as_str()).collect();
    assert!(sources.contains(&"note-a"));
    assert!(sources.contains(&"note-b"));
}

#[test]
fn test_delete_specific_link() {
    let s = make_links();
    s.create("del-src", "del-tgt").expect("create");
    s.delete("del-src", "del-tgt").expect("delete");

    let outbound = s.outbound_for("del-src").expect("outbound");
    assert!(outbound.is_empty());

    let inbound = s.inbound_for("del-tgt").expect("inbound");
    assert!(inbound.is_empty());
}

#[test]
fn test_delete_specific_link_not_found() {
    let s = make_links();
    let err = s
        .delete("missing-src", "missing-tgt")
        .expect_err("delete should fail");
    assert!(err.to_string().contains("missing-src"));
}

#[test]
fn test_delete_all_for() {
    let s = make_links();
    s.create("note-to-delete", "target-a").expect("link a");
    s.create("note-to-delete", "target-b").expect("link b");
    s.create("source-x", "note-to-delete").expect("link c");

    s.delete_all_for("note-to-delete").expect("delete_all_for");

    let outbound = s.outbound_for("note-to-delete").expect("outbound");
    assert!(outbound.is_empty());

    let inbound = s.inbound_for("note-to-delete").expect("inbound");
    assert!(inbound.is_empty());
}

#[test]
fn test_delete_all_for_only_deletes_links_for_that_slug() {
    let s = make_links();
    s.create("note-x", "note-y").expect("link 1");
    s.create("note-y", "note-z").expect("link 2");

    s.delete_all_for("note-y").expect("delete_all_for");

    // note-x -> note-y link should be gone
    assert!(s.outbound_for("note-x").expect("outbound").is_empty());
    // note-y -> note-z link should be gone
    assert!(s.outbound_for("note-y").expect("outbound").is_empty());
    // But check inbound for note-z is empty too
    assert!(s.inbound_for("note-z").expect("inbound").is_empty());
}

#[test]
fn test_create_is_idempotent() {
    // Inserting the same link twice should not error; it should be deduplicated.
    let s = make_links();
    s.create("dup-src", "dup-tgt").expect("first create");
    let second = s
        .create("dup-src", "dup-tgt")
        .expect("second create should not fail");

    let outbound = s.outbound_for("dup-src").expect("outbound");
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0].id, second.id);
}
