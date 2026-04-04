//! Sync engine integration tests covering merge logic and conflict resolution.
//!
//! These tests verify that [`SyncEngine`](scribe::sync::engine::SyncEngine)
//! correctly handles complex sync scenarios including:
//! - Bidirectional sync with remote having newer and local having newer data
//! - Multi-entity sync across all domain types
//! - Insert-only semantics for TimeEntry, Reminder, and CaptureItem
//! - Idempotent push avoidance when content hash is unchanged
//! - Full round-trip sync cycles

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use tempfile::TempDir;
use uuid::Uuid;

use scribe::domain::{
    CaptureItem, CaptureItemId, Project, ProjectId, ProjectStatus, Reminder, ReminderId, Task,
    TaskId, TaskPriority, TaskStatus, TimeEntry, TimeEntryId, Todo, TodoId,
};
use scribe::sync::engine::{SyncEngine, SyncState, SyncSummary};
use scribe::sync::snapshot::StateSnapshot;
use scribe::sync::{SyncError, SyncProvider};
use scribe::testing::keychain;

// ── test setup ────────────────────────────────────────────────────────────────

/// Initializes the mock keychain for sync tests.
fn setup() {
    keychain::use_mock_keychain();
}

// ── test fixtures ─────────────────────────────────────────────────────────────

/// Creates an empty [`StateSnapshot`] with no entities.
fn empty_snap() -> StateSnapshot {
    StateSnapshot {
        snapshot_at: Utc::now(),
        machine_id: Uuid::nil(),
        schema_version: StateSnapshot::SCHEMA_VERSION,
        projects: vec![],
        tasks: vec![],
        todos: vec![],
        time_entries: vec![],
        reminders: vec![],
        capture_items: vec![],
    }
}

/// Creates a project with the given slug, name, and updated_at timestamp.
fn make_project(slug: &str, name: &str, updated_secs_ago: i64) -> Project {
    let t = Utc::now() - Duration::seconds(updated_secs_ago);
    Project {
        id: ProjectId(1),
        slug: slug.to_owned(),
        name: name.to_owned(),
        description: None,
        status: ProjectStatus::Active,
        is_reserved: false,
        archived_at: None,
        created_at: t,
        updated_at: t,
    }
}

/// Creates a task linked to the given project.
fn make_task(slug: &str, project_slug: &str, updated_secs_ago: i64) -> Task {
    let t = Utc::now() - Duration::seconds(updated_secs_ago);
    Task {
        id: TaskId(1),
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        project_slug: project_slug.to_owned(),
        title: slug.to_owned(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        due_date: None,
        archived_at: None,
        created_at: t,
        updated_at: t,
    }
}

/// Creates a todo linked to the given project.
fn make_todo(slug: &str, project_slug: &str, updated_secs_ago: i64) -> Todo {
    let t = Utc::now() - Duration::seconds(updated_secs_ago);
    Todo {
        id: TodoId(1),
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        project_slug: project_slug.to_owned(),
        title: slug.to_owned(),
        done: false,
        archived_at: None,
        created_at: t,
        updated_at: t,
    }
}

/// Creates a time entry (uses insert-or-keep semantics, no updated_at conflict).
fn make_time_entry(slug: &str, project_slug: &str) -> TimeEntry {
    let now = Utc::now();
    TimeEntry {
        id: TimeEntryId(1),
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        project_slug: project_slug.to_owned(),
        task_id: None,
        task_slug: None,
        started_at: now - Duration::minutes(30),
        ended_at: Some(now),
        note: None,
        archived_at: None,
        created_at: now,
    }
}

/// Creates a reminder (uses insert-or-keep semantics, no updated_at conflict).
fn make_reminder(slug: &str, project_slug: &str) -> Reminder {
    Reminder {
        id: ReminderId(1),
        slug: slug.to_owned(),
        project_id: ProjectId(1),
        project_slug: project_slug.to_owned(),
        task_id: None,
        task_slug: None,
        remind_at: Utc::now() + Duration::hours(1),
        message: None,
        fired: false,
        persistent: false,
        archived_at: None,
        created_at: Utc::now(),
    }
}

/// Creates a capture item (uses insert-or-keep semantics).
fn make_capture_item(slug: &str) -> CaptureItem {
    CaptureItem {
        id: CaptureItemId(1),
        slug: slug.to_owned(),
        body: "Captured thought".to_owned(),
        processed: false,
        created_at: Utc::now(),
    }
}

/// An in-memory mock provider for testing sync behavior.
#[derive(Debug, Clone)]
struct MockProvider {
    stored: Arc<Mutex<Option<StateSnapshot>>>,
    push_count: Arc<Mutex<u32>>,
    should_fail_push: bool,
    should_fail_pull: bool,
    pull_error_message: Option<String>,
}

impl MockProvider {
    fn new() -> Self {
        Self {
            stored: Arc::new(Mutex::new(None)),
            push_count: Arc::new(Mutex::new(0)),
            should_fail_push: false,
            should_fail_pull: false,
            pull_error_message: None,
        }
    }

    /// Pre-loads the mock with remote state.
    fn with_remote_state(self, snap: StateSnapshot) -> Self {
        *self.stored.lock().unwrap() = Some(snap);
        self
    }

    /// Configures the mock to fail push operations.
    fn with_push_failure(mut self) -> Self {
        self.should_fail_push = true;
        self
    }

    /// Configures the mock to fail pull operations with a specific error message.
    fn with_pull_failure(mut self, msg: String) -> Self {
        self.should_fail_pull = true;
        self.pull_error_message = Some(msg);
        self
    }

    fn push_count(&self) -> u32 {
        *self.push_count.lock().unwrap()
    }
}

#[async_trait]
impl SyncProvider for MockProvider {
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        if self.should_fail_push {
            return Err(SyncError::Transport("mock push failure".to_owned()));
        }
        *self.stored.lock().unwrap() = Some(snapshot.clone());
        *self.push_count.lock().unwrap() += 1;
        Ok(())
    }

    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        if self.should_fail_pull {
            let msg = self
                .pull_error_message
                .clone()
                .unwrap_or_else(|| "mock pull failure".to_owned());
            return Err(SyncError::Transport(msg));
        }
        self.stored
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| SyncError::NotFound("no remote state".to_owned()))
    }
}

// ── SyncState tests ───────────────────────────────────────────────────────────

#[test]
fn sync_state_load_returns_default_when_file_missing() {
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let path = temp_dir.path().join("nonexistent-state.json");
    let state = SyncState::load(&path);
    assert!(state.last_sync_at.is_none());
    assert!(state.last_error.is_none());
    assert!(state.provider.is_none());
}

#[test]
fn sync_state_save_and_load_roundtrip() {
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let path = temp_dir.path().join("sync-state.json");

    let original = SyncState {
        last_sync_at: Some(Utc::now()),
        last_error: None,
        provider: Some("test-provider".to_owned()),
    };
    original.save(&path).expect("save should succeed");

    let loaded = SyncState::load(&path);
    assert_eq!(loaded.provider, original.provider);
    assert!(loaded.last_sync_at.is_some());
}

#[test]
fn sync_state_save_creates_parent_directories() {
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let path = temp_dir
        .path()
        .join("nested")
        .join("deep")
        .join("sync-state.json");

    let state = SyncState::default();
    state.save(&path).expect("save should succeed");

    assert!(path.exists());
    let loaded = SyncState::load(&path);
    assert!(loaded.last_error.is_none()); // Verify it loaded correctly
}

// ── SyncSummary tests ─────────────────────────────────────────────────────────

#[test]
fn sync_summary_total_pulled_counts_all_entity_types() {
    let mut summary = SyncSummary::default();
    summary.projects_added = 1;
    summary.tasks_added = 2;
    summary.todos_added = 3;
    summary.time_entries_added = 4;
    summary.reminders_added = 5;
    summary.capture_items_added = 6;

    assert_eq!(summary.total_pulled(), 1 + 2 + 3 + 4 + 5 + 6);
}

#[test]
fn sync_summary_empty_returns_zero_total() {
    let summary = SyncSummary::empty();
    assert_eq!(summary.total_pulled(), 0);
}

// ── merge_into: Project tests ─────────────────────────────────────────────────

#[test]
fn merge_remote_only_project_is_inserted() {
    let mut local = empty_snap();
    let mut remote = empty_snap();
    remote
        .projects
        .push(make_project("remote-only", "Remote Only", 0));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.projects.len(), 1);
    assert_eq!(local.projects[0].slug, "remote-only");
}

#[test]
fn merge_local_only_project_is_preserved() {
    let mut local = empty_snap();
    local
        .projects
        .push(make_project("local-only", "Local Only", 0));
    let remote = empty_snap();
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.projects.len(), 1);
    assert_eq!(local.projects[0].slug, "local-only");
}

#[test]
fn merge_remote_newer_project_replaces_local() {
    let mut local = empty_snap();
    local.projects.push(make_project("shared", "Old Name", 120)); // 2 minutes ago
    let mut remote = empty_snap();
    remote.projects.push(make_project("shared", "New Name", 10)); // 10 seconds ago
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.projects.len(), 1);
    assert_eq!(local.projects[0].name, "New Name");
}

#[test]
fn merge_local_newer_project_is_preserved() {
    let mut local = empty_snap();
    local.projects.push(make_project("shared", "New Name", 10)); // newer
    let mut remote = empty_snap();
    remote
        .projects
        .push(make_project("shared", "Old Name", 120)); // older
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.projects.len(), 1);
    assert_eq!(local.projects[0].name, "New Name");
}

#[test]
fn merge_same_timestamp_preserves_local() {
    let t = Utc::now() - Duration::seconds(60);
    let mut local = empty_snap();
    local.projects.push(Project {
        id: ProjectId(1),
        slug: "shared".to_owned(),
        name: "Local".to_owned(),
        description: None,
        status: ProjectStatus::Active,
        is_reserved: false,
        archived_at: None,
        created_at: t,
        updated_at: t,
    });
    let mut remote = empty_snap();
    remote.projects.push(Project {
        id: ProjectId(2), // different ID
        slug: "shared".to_owned(),
        name: "Remote".to_owned(),
        description: None,
        status: ProjectStatus::Active,
        is_reserved: false,
        archived_at: None,
        created_at: t,
        updated_at: t, // same timestamp
    });
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.projects.len(), 1);
    assert_eq!(local.projects[0].name, "Local"); // local preserved
}

#[test]
fn merge_archived_project_propagates_from_remote() {
    let mut local = empty_snap();
    local.projects.push(make_project("proj", "Project", 120));
    let mut remote = empty_snap();
    let mut archived = make_project("proj", "Project", 10);
    archived.archived_at = Some(Utc::now());
    remote.projects.push(archived);
    SyncEngine::merge_into(&mut local, &remote);
    assert!(local.projects[0].archived_at.is_some());
}

// ── merge_into: Task tests ───────────────────────────────────────────────────

#[test]
fn merge_remote_only_task_is_inserted() {
    let mut local = empty_snap();
    let mut remote = empty_snap();
    remote.tasks.push(make_task("remote-task", "proj", 0));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.tasks.len(), 1);
    assert_eq!(local.tasks[0].slug, "remote-task");
}

#[test]
fn merge_task_remote_newer_wins() {
    let mut local = empty_snap();
    local.tasks.push(make_task("shared-task", "proj", 120)); // older: 120 seconds ago
    let mut remote = empty_snap();
    remote.tasks.push(make_task("shared-task", "proj", 10)); // newer: 10 seconds ago
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.tasks.len(), 1);
    // After merge with remote being newer, local should have the remote's updated_at
    // The remote's updated_at is ~10 seconds ago.
    let task_updated_at = local.tasks[0].updated_at;
    let age_seconds = (Utc::now() - task_updated_at).num_seconds();
    assert!(
        age_seconds < 20,
        "merged task should have remote's recent timestamp (~10s), but is {} seconds old",
        age_seconds
    );
    assert!(
        age_seconds >= 5,
        "merged task should have remote's timestamp (~10s old), but seems too recent: {} seconds",
        age_seconds
    );
}

#[test]
fn merge_task_local_newer_preserved() {
    let mut local = empty_snap();
    local.tasks.push(make_task("shared-task", "proj", 10)); // newer: 10 seconds ago
    let mut remote = empty_snap();
    remote.tasks.push(make_task("shared-task", "proj", 120)); // older: 120 seconds ago
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.tasks.len(), 1);
    // Local is newer, so it should be preserved (local's updated_at is ~10 seconds ago)
    let task_updated_at = local.tasks[0].updated_at;
    let age_seconds = (Utc::now() - task_updated_at).num_seconds();
    assert!(
        age_seconds < 20,
        "merged task should have local's recent timestamp (~10s), but is {} seconds old",
        age_seconds
    );
    assert!(
        age_seconds >= 5,
        "merged task should have local's timestamp (~10s old), but seems too recent: {} seconds",
        age_seconds
    );
}

// ── merge_into: Todo tests ──────────────────────────────────────────────────

#[test]
fn merge_remote_only_todo_is_inserted() {
    let mut local = empty_snap();
    let mut remote = empty_snap();
    remote.todos.push(make_todo("remote-todo", "proj", 0));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.todos.len(), 1);
    assert_eq!(local.todos[0].slug, "remote-todo");
}

#[test]
fn merge_todo_remote_newer_wins() {
    let mut local = empty_snap();
    local.todos.push(make_todo("shared-todo", "proj", 120)); // older
    let mut remote = empty_snap();
    remote.todos.push(make_todo("shared-todo", "proj", 10)); // newer
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.todos.len(), 1);
    // After merge with remote being newer, local should have the remote's updated_at
    // The remote's updated_at is ~10 seconds ago.
    let todo_updated_at = local.todos[0].updated_at;
    let age_seconds = (Utc::now() - todo_updated_at).num_seconds();
    assert!(
        age_seconds < 20,
        "merged todo should have remote's recent timestamp (~10s), but is {} seconds old",
        age_seconds
    );
    assert!(
        age_seconds >= 5,
        "merged todo should have remote's timestamp (~10s old), but seems too recent: {} seconds",
        age_seconds
    );
}

// ── merge_into: TimeEntry tests (insert-or-keep semantics) ───────────────────

#[test]
fn merge_time_entry_remote_only_is_inserted() {
    let mut local = empty_snap();
    let mut remote = empty_snap();
    remote.time_entries.push(make_time_entry("entry-1", "proj"));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.time_entries.len(), 1);
}

#[test]
fn merge_time_entry_both_exist_local_preserved() {
    // TimeEntry uses insert-or-keep semantics - no updated_at comparison.
    let mut local = empty_snap();
    local
        .time_entries
        .push(make_time_entry("shared-entry", "proj"));
    let mut remote = empty_snap();
    remote
        .time_entries
        .push(make_time_entry("shared-entry", "proj"));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.time_entries.len(), 1);
}

#[test]
fn merge_multiple_time_entries_from_both_sides() {
    let mut local = empty_snap();
    local
        .time_entries
        .push(make_time_entry("local-entry", "proj"));
    let mut remote = empty_snap();
    remote
        .time_entries
        .push(make_time_entry("remote-entry", "proj"));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.time_entries.len(), 2);
    let slugs: Vec<_> = local.time_entries.iter().map(|e| e.slug.as_str()).collect();
    assert!(slugs.contains(&"local-entry"));
    assert!(slugs.contains(&"remote-entry"));
}

// ── merge_into: Reminder tests (insert-or-keep semantics) ───────────────────

#[test]
fn merge_reminder_remote_only_inserted() {
    let mut local = empty_snap();
    let mut remote = empty_snap();
    remote
        .reminders
        .push(make_reminder("remote-reminder", "proj"));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.reminders.len(), 1);
}

#[test]
fn merge_reminder_both_exist_local_preserved() {
    // Reminder uses insert-or-keep semantics.
    let mut local = empty_snap();
    local
        .reminders
        .push(make_reminder("shared-reminder", "proj"));
    let mut remote = empty_snap();
    remote
        .reminders
        .push(make_reminder("shared-reminder", "proj"));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.reminders.len(), 1);
}

// ── merge_into: CaptureItem tests (insert-or-keep semantics) ───────────────

#[test]
fn merge_capture_item_remote_only_inserted() {
    let mut local = empty_snap();
    let mut remote = empty_snap();
    remote
        .capture_items
        .push(make_capture_item("remote-capture"));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.capture_items.len(), 1);
}

#[test]
fn merge_capture_item_both_exist_local_preserved() {
    let mut local = empty_snap();
    local
        .capture_items
        .push(make_capture_item("shared-capture"));
    let mut remote = empty_snap();
    remote
        .capture_items
        .push(make_capture_item("shared-capture"));
    SyncEngine::merge_into(&mut local, &remote);
    assert_eq!(local.capture_items.len(), 1);
}

// ── merge_into: Multi-entity complex scenarios ──────────────────────────────

#[test]
fn merge_handles_all_entity_types_simultaneously() {
    let mut local = empty_snap();
    local.projects.push(make_project("local-proj", "Local", 10));
    local.tasks.push(make_task("local-task", "local-proj", 10));
    local.todos.push(make_todo("local-todo", "local-proj", 10));
    local
        .time_entries
        .push(make_time_entry("local-entry", "local-proj"));
    local
        .reminders
        .push(make_reminder("local-reminder", "local-proj"));
    local.capture_items.push(make_capture_item("local-capture"));

    let mut remote = empty_snap();
    remote
        .projects
        .push(make_project("remote-proj", "Remote", 10));
    remote
        .tasks
        .push(make_task("remote-task", "remote-proj", 10));
    remote
        .todos
        .push(make_todo("remote-todo", "remote-proj", 10));
    remote
        .time_entries
        .push(make_time_entry("remote-entry", "remote-proj"));
    remote
        .reminders
        .push(make_reminder("remote-reminder", "remote-proj"));
    remote
        .capture_items
        .push(make_capture_item("remote-capture"));

    SyncEngine::merge_into(&mut local, &remote);

    assert_eq!(local.projects.len(), 2);
    assert_eq!(local.tasks.len(), 2);
    assert_eq!(local.todos.len(), 2);
    assert_eq!(local.time_entries.len(), 2);
    assert_eq!(local.reminders.len(), 2);
    assert_eq!(local.capture_items.len(), 2);
}

#[test]
fn merge_conflict_across_all_entity_types_with_different_winners() {
    // Local is newer for projects, remote is newer for tasks, todos use insert-or-keep.
    let mut local = empty_snap();
    local.projects.push(make_project("proj", "Local Newer", 10));
    local.tasks.push(make_task("task", "proj", 120)); // older
    local.todos.push(make_todo("todo", "proj", 120)); // older

    let mut remote = empty_snap();
    remote
        .projects
        .push(make_project("proj", "Remote Older", 120)); // older
    remote.tasks.push(make_task("task", "proj", 10)); // newer
    remote.todos.push(make_todo("todo", "proj", 10)); // newer

    SyncEngine::merge_into(&mut local, &remote);

    // Project: local wins (newer)
    assert_eq!(local.projects[0].name, "Local Newer");
    // Task: remote wins (newer)
    assert!(local.tasks[0].updated_at > Utc::now() - Duration::seconds(60));
    // Todo: remote wins (newer) - last-write-wins applies
    assert!(local.todos[0].updated_at > Utc::now() - Duration::seconds(60));
}

// ── SyncEngine.run_once tests ────────────────────────────────────────────────

#[tokio::test]
async fn engine_run_once_pushes_when_remote_empty() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");
    let provider = MockProvider::new();
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());
    let local = empty_snap();

    let result = engine.run_once(local).await;
    assert!(result.is_ok(), "run_once should succeed, got: {result:?}");
    let (_merged, summary) = result.unwrap();
    assert!(summary.error.is_none());
}

#[tokio::test]
async fn engine_run_once_merges_remote_into_local() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    let remote = empty_snap();
    let provider = MockProvider::new().with_remote_state(remote);
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    let local = empty_snap();
    let (merged, _summary) = engine.run_once(local).await.unwrap();

    // The merged snapshot should have entities from remote.
    // Since both were empty, it should still be empty.
    assert_eq!(merged.entities(), 0);
}

#[tokio::test]
async fn engine_run_once_pulls_and_merges_preloaded_remote_state() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    let mut remote = empty_snap();
    remote
        .projects
        .push(make_project("remote-proj", "Remote", 0));

    let provider = MockProvider::new().with_remote_state(remote);
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    let local = empty_snap();
    let (merged, _summary) = engine.run_once(local).await.unwrap();

    assert_eq!(merged.projects.len(), 1);
    assert_eq!(merged.projects[0].slug, "remote-proj");
}

#[tokio::test]
async fn engine_run_once_skips_push_when_content_hash_unchanged() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    // Remote has the same content as local.
    let remote = empty_snap();
    let provider = MockProvider::new().with_remote_state(remote);
    let provider_for_assert = provider.clone();
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    let local = empty_snap();
    let (_merged, _summary) = engine.run_once(local).await.unwrap();

    // Since content hash is unchanged after merge, push should be skipped.
    // The mock's push_count will still be 0.
    assert_eq!(provider_for_assert.push_count(), 0);
}

#[tokio::test]
async fn engine_run_once_pushes_when_content_hash_changed() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    // Pre-load remote with a project.
    let remote = empty_snap();
    let provider = MockProvider::new().with_remote_state(remote);
    let provider_for_assert = provider.clone();
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    // Local also has a project but with different name - after merge,
    // the local project will be replaced by the remote project (since remote is newer
    // with updated_at = 0). This creates different content, triggering a push.
    let mut local = empty_snap();
    local
        .projects
        .push(make_project("local-proj", "Local Project", 120)); // older than remote

    engine.run_once(local).await.unwrap();

    // After merge, content changed (local older project replaced by remote newer empty
    // merged with remote having a project), so push should have been called.
    assert_eq!(provider_for_assert.push_count(), 1);
}

#[tokio::test]
async fn engine_run_once_returns_error_on_push_failure() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    let provider = MockProvider::new().with_push_failure();
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    let local = empty_snap();
    let result = engine.run_once(local).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), SyncError::Transport(_)));
}

#[tokio::test]
async fn engine_run_once_returns_error_on_pull_failure() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    let provider = MockProvider::new().with_pull_failure("network error".to_owned());
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    let local = empty_snap();
    let result = engine.run_once(local).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), SyncError::Transport(_)));
}

#[tokio::test]
async fn engine_run_once_notfound_pushes_initial_snapshot() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    // Provider returns NotFound (no remote state).
    let provider = MockProvider::new();
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    let local = empty_snap();
    let result = engine.run_once(local).await;

    assert!(
        result.is_ok(),
        "NotFound should not be an error, got: {result:?}"
    );
    let (_merged, summary) = result.unwrap();
    assert!(summary.error.is_none());
}

#[tokio::test]
async fn engine_run_once_full_merge_cycle() {
    setup();
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");

    // Pre-load remote with some data.
    let mut remote = empty_snap();
    remote
        .projects
        .push(make_project("remote-proj", "Remote Project", 0));
    remote
        .tasks
        .push(make_task("remote-task", "remote-proj", 0));

    let provider = MockProvider::new().with_remote_state(remote);
    let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());

    // Local has different data.
    let mut local = empty_snap();
    local
        .projects
        .push(make_project("local-proj", "Local Project", 0));

    let (merged, summary) = engine.run_once(local).await.unwrap();

    // Both projects should be present.
    assert_eq!(merged.projects.len(), 2);
    assert!(merged.projects.iter().any(|p| p.slug == "remote-proj"));
    assert!(merged.projects.iter().any(|p| p.slug == "local-proj"));

    // Remote task should be present (local had no tasks).
    assert_eq!(merged.tasks.len(), 1);

    // Summary should reflect the additions.
    assert!(summary.projects_added >= 1);
}

// ── SyncSummary::from_comparison tests ──────────────────────────────────────

#[test]
fn sync_summary_from_comparison_counts_additions_and_updates() {
    let original = empty_snap();
    let mut merged = empty_snap();
    merged.projects.push(make_project("new-proj", "New", 0));
    merged
        .projects
        .push(make_project("updated-proj", "Updated", 0));

    // Simulate an update by modifying an existing project's name.
    merged.projects[0].name = "Modified Name".to_owned();

    let summary = SyncSummary::from_comparison(&original, &merged);

    // Original had 0 projects, merged has 2.
    assert_eq!(summary.projects_added, 2);
    // No updates since original was empty.
    assert_eq!(summary.projects_updated, 0);
}

// ── Content hash tests ───────────────────────────────────────────────────────

#[test]
fn snapshot_content_hash_excludes_timestamp_and_machine_id() {
    let mut snap1 = empty_snap();
    let h1 = snap1.content_hash();

    // Change snapshot_at - hash should stay the same.
    snap1.snapshot_at = Utc::now() + Duration::days(1);
    let h2 = snap1.content_hash();
    assert_eq!(h1, h2, "content hash should not depend on snapshot_at");

    // Change machine_id - hash should stay the same.
    snap1.machine_id = Uuid::new_v4();
    let h3 = snap1.content_hash();
    assert_eq!(h1, h3, "content hash should not depend on machine_id");
}

#[test]
fn snapshot_content_hash_includes_all_entity_data() {
    let mut snap1 = empty_snap();
    let h1 = snap1.content_hash();

    // Add a project - hash should change.
    snap1.projects.push(make_project("proj", "Project", 0));
    let h2 = snap1.content_hash();
    assert_ne!(h1, h2, "adding entity should change content hash");

    // Clear projects - hash should go back to original.
    snap1.projects.clear();
    let h3 = snap1.content_hash();
    assert_eq!(h1, h3, "clearing entities should restore original hash");
}

#[test]
fn snapshot_content_hash_is_deterministic() {
    let snap = empty_snap();
    let h1 = snap.content_hash();
    let h2 = snap.content_hash();
    assert_eq!(h1, h2, "content hash should be deterministic");
}

// ── provider_name and sync_state_path accessors ──────────────────────────────

#[test]
fn engine_returns_provider_name() {
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");
    let provider = MockProvider::new();
    let engine = SyncEngine::new(Box::new(provider), state_path, "my-provider".to_owned());
    assert_eq!(engine.provider_name(), "my-provider");
}

#[test]
fn engine_returns_sync_state_path() {
    let temp_dir = TempDir::new().expect("tempdir should succeed");
    let state_path = temp_dir.path().join("sync-state.json");
    let provider = MockProvider::new();
    let engine = SyncEngine::new(Box::new(provider), state_path.clone(), "test".to_owned());
    assert_eq!(engine.sync_state_path(), state_path.as_path());
}
