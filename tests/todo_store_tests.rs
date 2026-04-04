//! Unit tests for [`crate::store::todo_store::SqliteTodos`].

use scribe::domain::{NewTodo, ProjectId, TodoPatch, Todos};
use scribe::testing::todo_store;

// Re-export the store testing helper for use within this module.
use todo_store::store as make_store;

fn new_todo(slug: &str, title: &str) -> NewTodo {
    NewTodo {
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        title: title.to_owned(),
    }
}

#[test]
fn test_create_and_find() {
    let s = make_store();
    let t = s.create(new_todo("t1", "Do thing")).expect("create");
    assert_eq!(t.slug, "t1");
    assert!(!t.done);
}

#[test]
fn test_mark_done() {
    let s = make_store();
    s.create(new_todo("td", "Do it")).expect("create");
    let updated = s
        .update(
            "td",
            TodoPatch {
                done: Some(true),
                ..Default::default()
            },
        )
        .expect("update");
    assert!(updated.done);
}

#[test]
fn test_archive_hide_from_list() {
    let s = make_store();
    s.create(new_todo("ta", "Archive me")).expect("archive");
    s.archive("ta").expect("archive");
    let items = s.list(None, true, false).expect("list");
    assert!(!items.iter().any(|t| t.slug == "ta"));
}
