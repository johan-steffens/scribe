//! Independent verification for snapshot schema v2 (hierarchical tasks + notes).
//!
//! Covers:
//! - `parent_slug` is portable across databases (`parent_id` is not)
//! - Outbound snapshots do not dual-represent checklist items as todos
//! - Notes round-trip through `from_db` / `write_to_db`
//! - Older task JSON without `kind` / `parent_slug` still deserialises

#![cfg(feature = "sync")]

use std::sync::{Arc, Mutex};

use chrono::Utc;
use uuid::Uuid;

use scribe::domain::{
    NewNote, NewProject, NewTask, NewTodo, Note, Notes, ProjectStatus, Projects, Task, TaskId,
    TaskKind, TaskPriority, TaskStatus, Tasks, Todos,
};
use scribe::store::{SqliteNotes, SqliteProjects, SqliteTasks, SqliteTodos};
use scribe::sync::snapshot::StateSnapshot;
use scribe::testing::db::TestDb;

fn open_db() -> (TestDb, Arc<Mutex<rusqlite::Connection>>) {
    let db = TestDb::new();
    let conn = Arc::clone(&db.conn());
    (db, conn)
}

#[test]
fn schema_version_is_v2() {
    assert_eq!(StateSnapshot::SCHEMA_VERSION, 2);
}

#[test]
fn task_deserialises_without_kind_or_parent_fields() {
    // Pre-v2 remote payload: no kind / parent_id / parent_slug.
    let json = r#"{
        "id": 1,
        "slug": "work-task-old",
        "project_id": 1,
        "project_slug": "work",
        "title": "Old task",
        "description": null,
        "status": "todo",
        "priority": "medium",
        "due_date": null,
        "archived_at": null,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    }"#;
    let task: Task = serde_json::from_str(json).expect("legacy task JSON must deserialise");
    assert_eq!(task.kind, TaskKind::Task);
    assert!(task.parent_id.is_none());
    assert!(task.parent_slug.is_none());
}

#[test]
fn outbound_snapshot_does_not_dual_write_checklist_items() {
    let (_db, conn) = open_db();
    let projects = SqliteProjects::new(Arc::clone(&conn));
    let tasks = SqliteTasks::new(Arc::clone(&conn));
    let todos = SqliteTodos::new(Arc::clone(&conn));

    let project = projects
        .create(NewProject {
            slug: "work".into(),
            name: "Work".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .unwrap();

    tasks
        .create(NewTask {
            slug: "work-task-parent".into(),
            project_id: project.id,
            title: "Parent".into(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
            kind: TaskKind::Task,
        })
        .unwrap();

    // Checklist item (former todo).
    todos
        .create(NewTodo {
            slug: "work-todo-item".into(),
            project_id: project.id,
            title: "Checklist".into(),
        })
        .unwrap();

    let snap = StateSnapshot::from_db(&conn, Uuid::nil()).unwrap();
    assert!(
        snap.todos.is_empty(),
        "outbound todos must be empty to avoid dual representation"
    );
    assert!(
        snap.tasks
            .iter()
            .any(|t| t.slug == "work-todo-item" && t.kind == TaskKind::ChecklistItem),
        "checklist items must appear under tasks with kind=checklist_item"
    );
    assert!(
        snap.tasks.iter().any(|t| t.slug == "work-task-parent"),
        "full tasks must appear under tasks"
    );
}

#[test]
fn parent_slug_survives_cross_db_write() {
    let (_db_a, conn_a) = open_db();
    let projects_a = SqliteProjects::new(Arc::clone(&conn_a));
    let tasks_a = SqliteTasks::new(Arc::clone(&conn_a));

    let project = projects_a
        .create(NewProject {
            slug: "work".into(),
            name: "Work".into(),
            description: None,
            status: ProjectStatus::Active,
        })
        .unwrap();

    let parent = tasks_a
        .create(NewTask {
            slug: "work-task-parent".into(),
            project_id: project.id,
            title: "Parent".into(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: None,
            kind: TaskKind::Task,
        })
        .unwrap();

    tasks_a
        .create(NewTask {
            slug: "work-task-child".into(),
            project_id: project.id,
            title: "Child".into(),
            description: None,
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            due_date: None,
            parent_id: Some(parent.id),
            kind: TaskKind::Task,
        })
        .unwrap();

    let snap = StateSnapshot::from_db(&conn_a, Uuid::nil()).unwrap();
    let child = snap
        .tasks
        .iter()
        .find(|t| t.slug == "work-task-child")
        .expect("child in snapshot");
    assert_eq!(child.parent_slug.as_deref(), Some("work-task-parent"));
    // parent_id is local to conn_a; deliberately wrong for a fresh DB.
    let mut portable = snap.clone();
    for t in &mut portable.tasks {
        t.parent_id = Some(TaskId(9999));
        t.project_id = scribe::domain::ProjectId(9999);
    }

    let (_db_b, conn_b) = open_db();
    portable.write_to_db(&conn_b).expect("write to second DB");

    let restored = StateSnapshot::from_db(&conn_b, Uuid::nil()).unwrap();
    let child_b = restored
        .tasks
        .iter()
        .find(|t| t.slug == "work-task-child")
        .expect("child restored");
    assert_eq!(child_b.parent_slug.as_deref(), Some("work-task-parent"));
    let parent_b = restored
        .tasks
        .iter()
        .find(|t| t.slug == "work-task-parent")
        .expect("parent restored");
    assert_eq!(
        child_b.parent_id,
        Some(parent_b.id),
        "parent_id must resolve via parent_slug on the destination DB"
    );
}

#[test]
fn notes_round_trip_through_snapshot() {
    let (_db, conn) = open_db();
    let notes = SqliteNotes::new(Arc::clone(&conn));
    notes
        .create(NewNote {
            slug: "arch-draft".into(),
            title: "Architecture".into(),
            content: "See [[work-task-parent]] for context.".into(),
        })
        .unwrap();

    let snap = StateSnapshot::from_db(&conn, Uuid::nil()).unwrap();
    assert_eq!(snap.notes.len(), 1);
    assert_eq!(snap.notes[0].slug, "arch-draft");

    let (_db2, conn2) = open_db();
    snap.write_to_db(&conn2).unwrap();
    let restored = StateSnapshot::from_db(&conn2, Uuid::nil()).unwrap();
    assert_eq!(restored.notes.len(), 1);
    assert_eq!(restored.notes[0].title, "Architecture");
    assert!(restored.notes[0].content.contains("[[work-task-parent]]"));
}

#[test]
fn snapshot_empty_notes_default_on_legacy_json() {
    let json = serde_json::json!({
        "snapshot_at": "2026-01-01T00:00:00Z",
        "machine_id": "00000000-0000-0000-0000-000000000000",
        "schema_version": 1,
        "projects": [],
        "tasks": [],
        "todos": [],
        "time_entries": [],
        "reminders": [],
        "capture_items": []
    });
    let snap: StateSnapshot =
        serde_json::from_value(json).expect("v1 snapshot without notes must deserialise");
    assert!(snap.notes.is_empty());
}

#[test]
fn note_lww_merge_prefers_newer_updated_at() {
    use scribe::sync::engine::SyncEngine;

    let older = Note {
        id: scribe::domain::NoteId(1),
        slug: "n1".into(),
        title: "old".into(),
        content: "old body".into(),
        created_at: Utc::now(),
        updated_at: Utc::now() - chrono::Duration::hours(2),
    };
    let newer = Note {
        id: scribe::domain::NoteId(2),
        slug: "n1".into(),
        title: "new".into(),
        content: "new body".into(),
        created_at: older.created_at,
        updated_at: Utc::now(),
    };

    let mut local = StateSnapshot {
        snapshot_at: Utc::now(),
        machine_id: Uuid::nil(),
        schema_version: StateSnapshot::SCHEMA_VERSION,
        projects: vec![],
        tasks: vec![],
        todos: vec![],
        time_entries: vec![],
        reminders: vec![],
        capture_items: vec![],
        notes: vec![older],
    };
    let remote = StateSnapshot {
        notes: vec![newer.clone()],
        ..local.clone()
    };
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.notes.len(), 1);
    assert_eq!(local.notes[0].title, "new");
    assert_eq!(local.notes[0].content, newer.content);
}
