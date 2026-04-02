// Rust guideline compliant 2026-02-21
//! Integration tests for the REST sync master server (gated behind `sync`).

#[cfg(feature = "sync")]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use scribe::server::start_server;
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

    #[tokio::test]
    async fn test_server_get_state_requires_auth() {
        let (port, _handle) = start_server(0, "test-secret".to_owned(), empty_snap()).await;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/state"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn test_server_get_state_returns_snapshot_with_valid_auth() {
        let (port, _handle) = start_server(0, "test-secret".to_owned(), empty_snap()).await;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/state"))
            .bearer_auth("test-secret")
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        let snap: StateSnapshot = resp.json().await.unwrap();
        assert_eq!(snap.schema_version, StateSnapshot::SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_server_put_state_merges_and_returns_merged() {
        let (port, _handle) = start_server(0, "test-secret".to_owned(), empty_snap()).await;
        let client = reqwest::Client::new();
        let snap = empty_snap();
        let resp = client
            .put(format!("http://127.0.0.1:{port}/state"))
            .bearer_auth("test-secret")
            .json(&snap)
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        let merged: StateSnapshot = resp.json().await.unwrap();
        assert_eq!(merged.schema_version, StateSnapshot::SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_server_put_state_requires_auth() {
        let (port, _handle) = start_server(0, "test-secret".to_owned(), empty_snap()).await;
        let client = reqwest::Client::new();
        let resp = client
            .put(format!("http://127.0.0.1:{port}/state"))
            // no auth header — should be rejected
            .json(&empty_snap())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
    }
}
