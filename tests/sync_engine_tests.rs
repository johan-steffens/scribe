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
}
