//! Cloud provider integration tests for sync providers.
//!
//! These tests verify that [`GistProvider`](scribe::sync::providers::gist::GistProvider)
//! and [`RestProvider`](scribe::sync::providers::rest::RestProvider) handle various
//! scenarios correctly.
//!
//! Note: Full wiremock-based tests for GistProvider and RestProvider require the
//! ability to inject mock keychain credentials, which is not currently supported.
//! The providers use the real `KeychainStore` for credential access.

use chrono::Utc;
use uuid::Uuid;

use scribe::domain::{Project, ProjectId, ProjectStatus};
use scribe::sync::providers::gist::GistProvider;
use scribe::sync::providers::rest::RestProvider;
use scribe::sync::snapshot::StateSnapshot;
use scribe::sync::{SyncError, SyncProvider};
use scribe::testing::keychain;

// ── test setup ────────────────────────────────────────────────────────────────

/// Initializes the mock keychain and returns a test snapshot.
fn setup() {
    keychain::use_mock_keychain();
    // Pre-load mock secrets so providers don't fail during initialization or pull.
    keychain::set_secret("rest", "secret", "test-secret").expect("failed to set rest secret");
    keychain::set_secret("gist", "token", "test-token").expect("failed to set gist token");
}

// ── test fixtures ─────────────────────────────────────────────────────────────

/// Creates a minimal [`StateSnapshot`] with no entities.
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

/// Creates a snapshot with one project for testing.
fn snap_with_project(slug: &str, name: &str) -> StateSnapshot {
    StateSnapshot {
        snapshot_at: Utc::now(),
        machine_id: Uuid::nil(),
        schema_version: StateSnapshot::SCHEMA_VERSION,
        projects: vec![Project {
            id: ProjectId(1),
            slug: slug.to_owned(),
            name: name.to_owned(),
            description: None,
            status: ProjectStatus::Active,
            is_reserved: false,
            archived_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }],
        tasks: vec![],
        todos: vec![],
        time_entries: vec![],
        reminders: vec![],
        capture_items: vec![],
    }
}

// ── Provider trait bounds tests ────────────────────────────────────────────────

#[test]
fn gist_provider_implements_send_and_sync() {
    setup();
    fn assert_send_sync<T: Send + Sync>() {}
    // This compile-time check verifies GistProvider satisfies Send + Sync.
    // GistProvider::new may fail if keychain is unavailable, but that's okay -
    // we're just verifying the type bounds.
    if let Ok(provider) = GistProvider::new(None) {
        assert_send_sync::<GistProvider>();
        let _ = provider; // suppress unused warning
    }
}

#[test]
fn rest_provider_implements_send_and_sync() {
    setup();
    fn assert_send_sync<T: Send + Sync>() {}
    // RestProvider::new doesn't require keychain access, so we can create it reliably.
    let _provider =
        RestProvider::new("http://localhost:9999").expect("RestProvider::new should succeed");
    assert_send_sync::<RestProvider>();
}

#[test]
fn rest_provider_new_accepts_various_url_formats() {
    // Test that RestProvider::new works with different URL formats.
    // This doesn't test network operations, just the constructor.
    let _provider = RestProvider::new("http://localhost:7171").expect("http URL should work");
    let _provider = RestProvider::new("http://127.0.0.1:8080").expect("IP address should work");
    let _provider = RestProvider::new("https://sync.example.com").expect("https URL should work");
}

// ── Network failure tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn rest_provider_returns_transport_error_on_connection_refused() {
    setup();
    // Use a port that's unlikely to have a server running.
    // This will fail at the TCP connection level, before any HTTP auth is needed.
    let provider =
        RestProvider::new("http://127.0.0.1:1").expect("RestProvider::new should succeed");
    let result = provider.pull().await;
    assert!(
        matches!(result, Err(SyncError::Transport(_))),
        "expected Transport error on connection refused, got: {result:?}"
    );
}

#[tokio::test]
async fn rest_provider_returns_transport_error_on_dns_failure() {
    setup();
    // Use an invalid hostname that will fail DNS resolution.
    // This fails before any HTTP request is made.
    let provider = RestProvider::new("http://this-domain-does-not-exist.invalid")
        .expect("RestProvider::new should succeed");
    let result = provider.pull().await;
    assert!(
        matches!(result, Err(SyncError::Transport(_))),
        "expected Transport error on DNS failure, got: {result:?}"
    );
}

// ── Snapshot serialisation tests ─────────────────────────────────────────────

#[test]
fn snapshot_serialises_and_deserialises_correctly() {
    let snap = snap_with_project("test-project", "Test Project");
    let json = serde_json::to_string(&snap).expect("snapshot should serialise");
    let deserialized: StateSnapshot =
        serde_json::from_str(&json).expect("snapshot should deserialise");
    assert_eq!(deserialized.projects.len(), 1);
    assert_eq!(deserialized.projects[0].slug, "test-project");
}

#[test]
fn empty_snapshot_has_zero_entities() {
    let snap = empty_snap();
    assert_eq!(snap.entities(), 0);
    assert_eq!(snap.projects.len(), 0);
    assert_eq!(snap.tasks.len(), 0);
    assert_eq!(snap.todos.len(), 0);
    assert_eq!(snap.time_entries.len(), 0);
    assert_eq!(snap.reminders.len(), 0);
    assert_eq!(snap.capture_items.len(), 0);
}
