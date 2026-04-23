//! Unit tests for [`crate::ops::notes::NotesOps`].

use std::env;
use std::path::PathBuf;
use std::sync::Mutex;

use scribe::domain::{NewNote, Notes};
use scribe::ops::NotesOps;
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
        let store = scribe::testing::note_store::notes_store();
        store
            .create(NewNote {
                slug: "existing-note".to_owned(),
                title: "Existing Note".to_owned(),
                content: "Original content".to_owned(),
            })
            .expect("create note in store");

        let ops = NotesOps::new(std::sync::Arc::new(store));
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
    let store = scribe::testing::note_store::notes_store();
    store
        .create(NewNote {
            slug: "get-test-note".to_owned(),
            title: "Get Test".to_owned(),
            content: "Hello".to_owned(),
        })
        .expect("create");

    let ops = NotesOps::new(std::sync::Arc::new(store));
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
    let store = scribe::testing::note_store::notes_store();
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

    let ops = NotesOps::new(std::sync::Arc::new(store));
    let all = ops.list().expect("list");
    assert_eq!(all.len(), 2);
}
