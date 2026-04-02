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
        snap.snapshot_at = snap.snapshot_at + chrono::Duration::seconds(1);
        let h2 = snap.content_hash();
        assert_eq!(h1, h2, "content hash should not depend on snapshot_at");
    }
}
