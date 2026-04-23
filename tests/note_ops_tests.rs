//! Unit tests for [`crate::ops::notes::NotesOps`].

use std::env;
use std::path::PathBuf;
use std::sync::Mutex;

use scribe::domain::{Links, NewNote, Notes};
use scribe::ops::NotesOps;
use scribe::store::note_store::testing::{links_store as make_links, notes_store as make_notes};
use scribe::testing::notes_ops;

use notes_ops::ops as make_ops;

// Serializes access to the global EDITOR env var during tests.
static EDITOR_MTX: Mutex<()> = Mutex::new(());

/// Returns the path to a test editor script that appends a suffix to the file.
fn test_editor_path() -> PathBuf {
    PathBuf::from("/tmp").join("scribe-test-editor.sh")
}

fn write_test_editor(suffix: &str) {
    let path = test_editor_path();
    // Shell script that appends suffix to the file passed as argument.
    // Using /bin/sh for maximum portability.
    let script = format!("#!/bin/sh\ncat >> \"$1\"\necho '{suffix}' >> \"$1\"\n");
    std::fs::write(&path, script).unwrap();
    // On Unix, make the script executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn cleanup_test_editor() {
    let _ = std::fs::remove_file(test_editor_path());
}

fn with_test_editor_env(suffix: &str, f: impl FnOnce()) {
    // Serialize test editor access to avoid races on the global EDITOR var.
    let _guard = EDITOR_MTX.lock().unwrap();
    write_test_editor(suffix);
    let old_editor = env::var("EDITOR").ok();
    // SAFETY: setting environment variables is inherently unsafe on some
    // platforms, but test editor paths are confined to the test process.
    unsafe { env::set_var("EDITOR", test_editor_path().to_str().unwrap()) };
    f();
    // SAFETY: restoring or removing the EDITOR var is safe for the same reason.
    unsafe {
        if let Some(v) = old_editor {
            env::set_var("EDITOR", v);
        } else {
            env::remove_var("EDITOR");
        }
    }
    cleanup_test_editor();
}

#[test]
fn test_create_and_edit_persists_content() {
    with_test_editor_env(" updated content", || {
        let ops = make_ops();
        let note = ops
            .create_and_edit("Test Note", "test-note-001")
            .expect("create and edit");

        // The test editor appends " updated content" to the file.
        assert!(note.content.contains("updated content"));
    });
}

#[test]
fn test_edit_note_updates_existing() {
    with_test_editor_env(" appended via edit", || {
        let store = make_notes();
        store
            .create(NewNote {
                slug: "existing-note".to_owned(),
                title: "Existing Note".to_owned(),
                content: "Original content".to_owned(),
            })
            .expect("create note in store");

        let ops = NotesOps::new(
            std::sync::Arc::new(store),
            std::sync::Arc::new(make_links()),
        );
        let edited = ops.edit_note("existing-note").expect("edit note");

        assert!(edited.content.contains("Original content"));
        assert!(edited.content.contains("appended via edit"));
    });
}

#[test]
fn test_edit_note_not_found_returns_error() {
    let ops = make_ops();
    let err = ops.edit_note("does-not-exist").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_get_returns_note() {
    let store = make_notes();
    store
        .create(NewNote {
            slug: "get-test-note".to_owned(),
            title: "Get Test".to_owned(),
            content: "Hello".to_owned(),
        })
        .expect("create");

    let ops = NotesOps::new(
        std::sync::Arc::new(store),
        std::sync::Arc::new(make_links()),
    );
    let found = ops.get("get-test-note").expect("get").expect("note exists");
    assert_eq!(found.slug, "get-test-note");
    assert_eq!(found.content, "Hello");
}

#[test]
fn test_get_not_found_returns_none() {
    let ops = make_ops();
    assert!(ops.get("nonexistent").expect("get").is_none());
}

#[test]
fn test_list_returns_all_notes() {
    let store = make_notes();
    store
        .create(NewNote {
            slug: "list-a".to_owned(),
            title: "A".to_owned(),
            content: String::new(),
        })
        .expect("create a");
    store
        .create(NewNote {
            slug: "list-b".to_owned(),
            title: "B".to_owned(),
            content: String::new(),
        })
        .expect("create b");

    let ops = NotesOps::new(
        std::sync::Arc::new(store),
        std::sync::Arc::new(make_links()),
    );
    let all = ops.list().expect("list");
    assert_eq!(all.len(), 2);
}

// ── Link parsing and sync tests ─────────────────────────────────────────────────

#[test]
fn test_parse_links_single() {
    let slugs = scribe::domain::parse_links("See [[my-task]] for details");
    assert_eq!(slugs, &["my-task"]);
}

#[test]
fn test_parse_links_multiple() {
    let slugs = scribe::domain::parse_links("[[link-a]] and [[link-b]]");
    assert_eq!(slugs, &["link-a", "link-b"]);
}

#[test]
fn test_parse_links_none() {
    let slugs = scribe::domain::parse_links("no links here");
    assert!(slugs.is_empty());
}

#[test]
fn test_parse_links_duplicates() {
    // Duplicates should be deduplicated (first occurrence wins).
    let slugs = scribe::domain::parse_links("[[a]] then [[b]] then [[a]] again");
    assert_eq!(slugs, &["a", "b"]);
}

#[test]
fn test_parse_links_mixed_content() {
    let slugs = scribe::domain::parse_links(
        "# Heading\n\nSome text [[my-task]] more text [[another-task]]",
    );
    assert_eq!(slugs, &["my-task", "another-task"]);
}

#[test]
fn test_parse_links_invalid_patterns() {
    // Single brackets should not match.
    let slugs = scribe::domain::parse_links("[single] and [[valid]]");
    assert_eq!(slugs, &["valid"]);
}

#[test]
fn test_parse_links_empty_brackets() {
    let slugs = scribe::domain::parse_links("[[]] and [[valid-task]]");
    assert_eq!(slugs, &["valid-task"]);
}

#[test]
fn test_sync_links_creates_outbound_links() {
    let ops = make_ops();
    ops.sync_links("source-note", "References [[target-a]] and [[target-b]]")
        .expect("sync links");

    let outbound = ops
        .links_store()
        .outbound_for("source-note")
        .expect("outbound");
    assert_eq!(outbound.len(), 2);
    let targets: Vec<_> = outbound.iter().map(|l| l.target_slug.as_str()).collect();
    assert!(targets.contains(&"target-a"));
    assert!(targets.contains(&"target-b"));
}

#[test]
fn test_sync_links_removes_old_links() {
    let ops = make_ops();
    // First sync with two links.
    ops.sync_links("source-note", "[[old-link]]")
        .expect("first sync");

    // Sync again with different links.
    ops.sync_links("source-note", "[[new-link]]")
        .expect("second sync");

    let outbound = ops
        .links_store()
        .outbound_for("source-note")
        .expect("outbound");
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0].target_slug, "new-link");
}

#[test]
fn test_sync_links_skips_self_reference() {
    let ops = make_ops();
    // Content references itself.
    ops.sync_links("my-note", "See [[my-note]] for details")
        .expect("sync links");

    let outbound = ops.links_store().outbound_for("my-note").expect("outbound");
    assert!(
        outbound.is_empty(),
        "self-referential link should be skipped"
    );
}

#[test]
fn test_sync_links_empty_content() {
    let ops = make_ops();
    // First create a link.
    ops.sync_links("source-note", "[[some-link]]")
        .expect("first sync");
    // Then sync with empty content.
    ops.sync_links("source-note", "").expect("second sync");

    let outbound = ops
        .links_store()
        .outbound_for("source-note")
        .expect("outbound");
    assert!(outbound.is_empty());
}

#[test]
fn test_edit_note_syncs_links() {
    with_test_editor_env(" [[new-target]]", || {
        let store = make_notes();
        store
            .create(NewNote {
                slug: "link-edit-test".to_owned(),
                title: "Link Edit Test".to_owned(),
                content: "Original [[old-target]]".to_owned(),
            })
            .expect("create note");

        let ops = NotesOps::new(
            std::sync::Arc::new(store),
            std::sync::Arc::new(make_links()),
        );

        // Pre-existing link.
        ops.sync_links("link-edit-test", "Original [[old-target]]")
            .expect("initial sync");

        // Verify old link exists.
        let outbound = ops
            .links_store()
            .outbound_for("link-edit-test")
            .expect("outbound");
        assert_eq!(outbound.len(), 1);
        assert_eq!(outbound[0].target_slug, "old-target");

        // Edit the note (editor appends " [[new-target]]").
        // Content becomes: "Original [[old-target]] [[new-target]]"
        let edited = ops.edit_note("link-edit-test").expect("edit");
        assert!(edited.content.contains("[[new-target]]"));

        // Verify links are updated - both old and new targets are now linked.
        let outbound = ops
            .links_store()
            .outbound_for("link-edit-test")
            .expect("outbound");
        assert_eq!(outbound.len(), 2);
        let targets: Vec<_> = outbound.iter().map(|l| l.target_slug.as_str()).collect();
        assert!(targets.contains(&"old-target"));
        assert!(targets.contains(&"new-target"));
    });
}
