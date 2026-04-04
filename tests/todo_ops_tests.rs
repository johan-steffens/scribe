// Rust guideline compliant 2026-02-21
//! Unit tests for [`crate::ops::todos::TodoOps`].

use scribe::testing::todo_ops;

use todo_ops::ops as make_ops;

#[test]
fn test_create_generates_slug() {
    let ops = make_ops();
    let todo = ops
        .create("quick-capture", "Buy groceries")
        .expect("create");
    assert_eq!(todo.slug, "quick-capture-todo-buy-groceries");
}

#[test]
fn test_create_project_not_found_returns_error() {
    let ops = make_ops();
    let err = ops.create("nonexistent", "Title").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_mark_done() {
    let ops = make_ops();
    let todo = ops.create("quick-capture", "Do thing").expect("create");
    let done = ops.mark_done(&todo.slug).expect("done");
    assert!(done.done);
}

#[test]
fn test_list_excludes_done_by_default() {
    let ops = make_ops();
    let todo = ops.create("quick-capture", "List test").expect("create");
    ops.mark_done(&todo.slug).expect("done");
    let active = ops.list(None, false, false).expect("list");
    assert!(!active.iter().any(|t| t.slug == todo.slug));
}

#[test]
fn test_list_includes_done_when_requested() {
    let ops = make_ops();
    let todo = ops.create("quick-capture", "Include done").expect("create");
    ops.mark_done(&todo.slug).expect("done");
    let all = ops.list(None, true, false).expect("list");
    assert!(all.iter().any(|t| t.slug == todo.slug));
}

#[test]
fn test_archive_and_restore() {
    let ops = make_ops();
    let todo = ops.create("quick-capture", "Archive me").expect("create");
    ops.archive(&todo.slug).expect("archive");
    let active = ops.list(None, true, false).expect("list");
    assert!(!active.iter().any(|t| t.slug == todo.slug));
    ops.restore(&todo.slug).expect("restore");
    let active = ops.list(None, true, false).expect("list");
    assert!(active.iter().any(|t| t.slug == todo.slug));
}

#[test]
fn test_delete_requires_archived() {
    let ops = make_ops();
    let todo = ops.create("quick-capture", "Delete me").expect("create");
    let err = ops.delete(&todo.slug).unwrap_err();
    assert!(err.to_string().contains("archived"));
}

#[test]
fn test_delete_archived_succeeds() {
    let ops = make_ops();
    let todo = ops
        .create("quick-capture", "Delete archived")
        .expect("create");
    ops.archive(&todo.slug).expect("archive");
    ops.delete(&todo.slug).expect("delete");
    assert!(ops.get(&todo.slug).expect("get").is_none());
}

#[test]
fn test_mark_undone() {
    let ops = make_ops();
    let todo = ops.create("quick-capture", "Mark undone").expect("create");
    ops.mark_done(&todo.slug).expect("mark done");
    let undone = ops.mark_undone(&todo.slug).expect("mark undone");
    assert!(!undone.done);
}

#[test]
fn test_update_title() {
    let ops = make_ops();
    let todo = ops.create("quick-capture", "Old title").expect("create");
    let updated = ops
        .update_title(&todo.slug, "New title")
        .expect("update title");
    assert_eq!(updated.title, "New title");
}
