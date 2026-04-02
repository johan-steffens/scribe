// Rust guideline compliant 2026-02-21
//! Integration tests for the sync engine (gated behind the `sync` feature).

#[cfg(feature = "sync")]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use scribe::sync::snapshot::StateSnapshot;

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

    #[test]
    fn test_snapshot_content_hash_is_stable() {
        let snap = empty_snap();
        let h1 = snap.content_hash();
        let h2 = snap.content_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_snapshot_content_hash_changes_when_data_changes() {
        let mut snap = empty_snap();
        let h1 = snap.content_hash();
        snap.schema_version = 2;
        let h2 = snap.content_hash();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_snapshot_content_hash_ignores_metadata() {
        let mut snap = empty_snap();
        let h1 = snap.content_hash();
        // Changing snapshot_at should NOT change the content hash
        snap.snapshot_at += chrono::Duration::seconds(1);
        let h2 = snap.content_hash();
        assert_eq!(h1, h2, "content hash should not depend on snapshot_at");
        // Changing machine_id should NOT change the content hash either
        snap.machine_id = uuid::Uuid::new_v4();
        let h3 = snap.content_hash();
        assert_eq!(h1, h3, "content hash should not depend on machine_id");
    }

    use async_trait::async_trait;
    use scribe::sync::{SyncError, SyncProvider};

    struct MockProvider {
        stored: std::sync::Mutex<Option<StateSnapshot>>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                stored: std::sync::Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl SyncProvider for MockProvider {
        async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
            *self.stored.lock().unwrap() = Some(snapshot.clone());
            Ok(())
        }

        async fn pull(&self) -> Result<StateSnapshot, SyncError> {
            self.stored
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| SyncError::NotFound("no remote state".to_owned()))
        }
    }

    #[tokio::test]
    async fn test_mock_provider_push_pull_roundtrip() {
        let provider = MockProvider::new();
        let snap = empty_snap();
        provider.push(&snap).await.unwrap();
        let pulled = provider.pull().await.unwrap();
        assert_eq!(pulled.schema_version, StateSnapshot::SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_mock_provider_pull_when_empty_returns_not_found() {
        let provider = MockProvider::new();
        let result = provider.pull().await;
        assert!(matches!(result, Err(SyncError::NotFound(_))));
    }

    use scribe::sync::providers::file::FileProvider;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_provider_push_then_pull_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let provider = FileProvider::new(path.clone());

        let snap = empty_snap();
        provider.push(&snap).await.unwrap();

        assert!(path.exists(), "state.json should exist after push");

        let pulled = provider.pull().await.unwrap();
        assert_eq!(pulled.schema_version, StateSnapshot::SCHEMA_VERSION);
        assert!(pulled.projects.is_empty());
    }

    #[tokio::test]
    async fn test_file_provider_pull_when_no_file_returns_not_found() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let provider = FileProvider::new(path);

        let result = provider.pull().await;
        assert!(
            matches!(result, Err(scribe::sync::SyncError::NotFound(_))),
            "expected NotFound, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_file_provider_push_creates_parent_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("state.json");
        let provider = FileProvider::new(path.clone());

        provider.push(&empty_snap()).await.unwrap();
        assert!(path.exists());
    }

    use scribe::sync::keychain::KeychainStore;

    #[test]
    fn test_keychain_service_name_format() {
        assert_eq!(
            KeychainStore::service_name("gist", "token"),
            "scribe.sync.gist.token"
        );
    }

    #[test]
    fn test_keychain_service_name_different_providers() {
        assert_eq!(
            KeychainStore::service_name("s3", "access_key_id"),
            "scribe.sync.s3.access_key_id"
        );
        assert_eq!(
            KeychainStore::service_name("rest", "secret"),
            "scribe.sync.rest.secret"
        );
    }

    // ── SyncEngine / merge_into tests ──────────────────────────────────────

    use chrono::Duration;
    use scribe::domain::{Project, ProjectId, ProjectStatus};
    use scribe::sync::engine::SyncEngine;

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

    #[test]
    fn test_merge_remote_only_entity_is_inserted() {
        let mut local = empty_snap();
        let mut remote = empty_snap();
        remote
            .projects
            .push(make_project("remote-proj", "Remote", 0));
        SyncEngine::merge_into(&mut local, &remote);
        assert_eq!(local.projects.len(), 1);
        assert_eq!(local.projects[0].slug, "remote-proj");
    }

    #[test]
    fn test_merge_local_only_entity_is_preserved() {
        let mut local = empty_snap();
        local.projects.push(make_project("local-proj", "Local", 0));
        let remote = empty_snap();
        SyncEngine::merge_into(&mut local, &remote);
        assert_eq!(local.projects.len(), 1);
        assert_eq!(local.projects[0].slug, "local-proj");
    }

    #[test]
    fn test_merge_remote_newer_replaces_local() {
        let mut local = empty_snap();
        local.projects.push(make_project("shared", "Old Name", 120));
        let mut remote = empty_snap();
        remote.projects.push(make_project("shared", "New Name", 10));
        SyncEngine::merge_into(&mut local, &remote);
        assert_eq!(local.projects.len(), 1);
        assert_eq!(local.projects[0].name, "New Name");
    }

    #[test]
    fn test_merge_local_newer_keeps_local() {
        let mut local = empty_snap();
        local.projects.push(make_project("shared", "New Name", 10));
        let mut remote = empty_snap();
        remote
            .projects
            .push(make_project("shared", "Old Name", 120));
        SyncEngine::merge_into(&mut local, &remote);
        assert_eq!(local.projects.len(), 1);
        assert_eq!(local.projects[0].name, "New Name");
    }

    #[test]
    fn test_merge_archived_propagates_from_remote() {
        let mut local = empty_snap();
        local.projects.push(make_project("proj", "Project", 120));
        let mut remote = empty_snap();
        let mut archived = make_project("proj", "Project", 10);
        archived.archived_at = Some(Utc::now());
        remote.projects.push(archived);
        SyncEngine::merge_into(&mut local, &remote);
        assert!(local.projects[0].archived_at.is_some());
    }

    #[tokio::test]
    async fn test_run_once_pushes_local_when_remote_empty() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("sync-state.json");
        let provider = MockProvider::new();
        let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());
        let local = empty_snap();
        let result = engine.run_once(local).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_from_config_returns_none_when_sync_disabled() {
        use scribe::sync::providers::from_config;
        let config = scribe::config::Config::default(); // sync.enabled = false by default
        let result = from_config(&config).unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_run_once_merges_remote_into_local() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("sync-state.json");
        let provider = MockProvider::new();
        // Pre-load remote state
        let mut remote = empty_snap();
        remote
            .projects
            .push(make_project("remote-proj", "Remote", 0));
        provider.push(&remote).await.unwrap();

        let engine = SyncEngine::new(Box::new(provider), state_path, "mock".to_owned());
        let local = empty_snap();
        let merged = engine.run_once(local).await.unwrap();
        assert_eq!(merged.projects.len(), 1);
        assert_eq!(merged.projects[0].slug, "remote-proj");
    }
}
