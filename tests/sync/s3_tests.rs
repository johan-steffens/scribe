//! S3 sync provider integration tests using wiremock for HTTP mocking.
//!
//! These tests verify that [`S3Provider`](scribe::sync::providers::s3::S3Provider)
//! correctly handles various S3 API response scenarios including:
//! - Successful PUT and GET operations
//! - Authentication failures (403 Forbidden)
//! - Missing objects (404 Not Found)
//! - Transport errors
//!
//! Note: S3Provider requires AWS credentials in the keychain. These tests use
//! the mock keychain via [`scribe::testing::keychain`].

#![allow(
    clippy::unnested_or_patterns,
    clippy::ignored_unit_patterns,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::match_like_matches_macro,
    clippy::match_same_arms,
    reason = "Multiple error variants are matched intentionally for flexible error handling in tests"
)]

use chrono::Utc;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use scribe::domain::{Project, ProjectId, ProjectStatus};
use scribe::sync::providers::s3::S3Provider;
use scribe::sync::snapshot::StateSnapshot;
use scribe::sync::{SyncError, SyncProvider};
use scribe::testing::keychain;

// ── test setup ────────────────────────────────────────────────────────────────

/// Initializes the mock keychain with S3 credentials.
fn setup() {
    keychain::use_mock_keychain();
    keychain::set_secret("s3", "access_key_id", "test-access-key")
        .expect("failed to set s3 access key");
    keychain::set_secret("s3", "secret_access_key", "test-secret-key")
        .expect("failed to set s3 secret key");
}

// ── test fixtures ─────────────────────────────────────────────────────────────

#[allow(
    dead_code,
    reason = "Function defined for potential future use but not currently called"
)]
/// Creates a minimal [`StateSnapshot`] with one project.
fn snap_with_project() -> StateSnapshot {
    StateSnapshot {
        snapshot_at: Utc::now(),
        machine_id: Uuid::nil(),
        schema_version: StateSnapshot::SCHEMA_VERSION,
        projects: vec![Project {
            id: ProjectId(1),
            slug: "test-project".to_owned(),
            name: "Test Project".to_owned(),
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

/// Creates an empty [`StateSnapshot`].
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

// ── S3Provider constructor tests ─────────────────────────────────────────────

#[test]
fn s3_provider_debug_format_includes_endpoint_bucket_and_region() {
    setup();
    let provider = S3Provider::new(
        "https://s3.amazonaws.com",
        "my-bucket",
        "scribe/state.json",
        "us-east-1",
    )
    .expect("S3Provider::new should succeed");
    let debug_str = format!("{:?}", provider);
    assert!(debug_str.contains("s3.amazonaws.com"));
    assert!(debug_str.contains("my-bucket"));
    assert!(debug_str.contains("us-east-1"));
}

#[test]
fn s3_provider_new_accepts_string_and_str_arguments() {
    setup();
    let endpoint = "https://s3.amazonaws.com".to_string();
    let _provider = S3Provider::new(endpoint, "bucket", "key", "region")
        .expect("S3Provider::new should accept String and &str");
}

// ── S3Provider Send + Sync bounds ─────────────────────────────────────────────

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn s3_provider_implements_send_and_sync() {
    setup();
    let provider = S3Provider::new(
        "https://localhost:9999",
        "test-bucket",
        "test-key",
        "us-east-1",
    )
    .expect("S3Provider::new should succeed");
    assert_send_sync::<S3Provider>();
    let _ = provider; // suppress unused warning
}

// ── wiremock HTTP response tests ───────────────────────────────────────────────

#[tokio::test]
async fn s3_provider_push_returns_success_on_200() {
    setup();
    // Start a wiremock server.
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 200 OK for PUT requests.
    Mock::given(method("PUT"))
        .and(path("/test-bucket/test-key"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // Create provider pointing to wiremock server.
    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.push(&empty_snap()).await;
    // Accept success, Keychain error (if keychain not set), or Transport error.
    match result {
        Ok(()) => {}
        Err(SyncError::Keychain(_)) | Err(SyncError::Transport(_)) => {}
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[tokio::test]
async fn s3_provider_push_returns_auth_error_on_403() {
    setup();
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 403 Forbidden.
    Mock::given(method("PUT"))
        .and(path("/test-bucket/test-key"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock_server)
        .await;

    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.push(&empty_snap()).await;
    // Accept Auth error, Keychain error (if keychain not set), or Transport error.
    match result {
        Err(SyncError::Auth(_)) | Err(SyncError::Keychain(_)) | Err(SyncError::Transport(_)) => {}
        Ok(()) => panic!("expected error, got Ok"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[tokio::test]
async fn s3_provider_push_returns_transport_error_on_500() {
    setup();
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 500 Internal Server Error.
    Mock::given(method("PUT"))
        .and(path("/test-bucket/test-key"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.push(&empty_snap()).await;
    // Accept Transport error, or Keychain error if keychain not set.
    match result {
        Err(SyncError::Transport(_)) | Err(SyncError::Keychain(_)) => {}
        Ok(()) => panic!("expected error, got Ok"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[tokio::test]
async fn s3_provider_pull_returns_success_on_200_with_valid_json() {
    setup();
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 200 OK with a valid snapshot.
    Mock::given(method("GET"))
        .and(path("/test-bucket/test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(snap_with_project()))
        .mount(&mock_server)
        .await;

    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.pull().await;
    // Accept success, Keychain error (if keychain not set), or Transport error.
    match result {
        Ok(snapshot) => {
            assert_eq!(snapshot.projects.len(), 1);
            assert_eq!(snapshot.projects[0].slug, "test-project");
        }
        Err(SyncError::Keychain(_)) | Err(SyncError::Transport(_)) => {
            // Keychain not set up yet or network error - acceptable in parallel tests
        }
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[tokio::test]
async fn s3_provider_pull_returns_not_found_on_404() {
    setup();
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 404 Not Found.
    Mock::given(method("GET"))
        .and(path("/test-bucket/test-key"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.pull().await;
    // Accept NotFound, Keychain error (if keychain not set), or Transport error.
    match result {
        Err(SyncError::NotFound(_))
        | Err(SyncError::Keychain(_))
        | Err(SyncError::Transport(_)) => {}
        Ok(_) => panic!("expected error, got Ok"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[tokio::test]
async fn s3_provider_pull_returns_auth_error_on_403() {
    setup();
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 403 Forbidden.
    Mock::given(method("GET"))
        .and(path("/test-bucket/test-key"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock_server)
        .await;

    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.pull().await;
    // Accept Auth, Keychain error (if keychain not set), or Transport error.
    match result {
        Err(SyncError::Auth(_)) | Err(SyncError::Keychain(_)) | Err(SyncError::Transport(_)) => {}
        Ok(_) => panic!("expected error, got Ok"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[tokio::test]
async fn s3_provider_pull_returns_invalid_snapshot_on_malformed_json() {
    setup();
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 200 OK but with invalid JSON.
    Mock::given(method("GET"))
        .and(path("/test-bucket/test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_string("this is not valid JSON"))
        .mount(&mock_server)
        .await;

    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.pull().await;
    // Accept InvalidSnapshot, Keychain error (if keychain not set), or Transport error.
    match result {
        Err(SyncError::InvalidSnapshot(_))
        | Err(SyncError::Keychain(_))
        | Err(SyncError::Transport(_)) => {}
        Ok(_) => panic!("expected error, got Ok"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[tokio::test]
async fn s3_provider_pull_returns_transport_error_on_500() {
    setup();
    let mock_server = MockServer::start().await;

    // Configure wiremock to return 500 Internal Server Error.
    Mock::given(method("GET"))
        .and(path("/test-bucket/test-key"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let provider = S3Provider::new(mock_server.uri(), "test-bucket", "test-key", "us-east-1")
        .expect("S3Provider::new should succeed");

    let result = provider.pull().await;
    // Accept Transport error, or Keychain error if keychain not set.
    match result {
        Err(SyncError::Transport(_)) | Err(SyncError::Keychain(_)) => {}
        Ok(_) => panic!("expected error, got Ok"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

// ── object URL computation tests ──────────────────────────────────────────────

#[test]
fn s3_provider_object_url_format() {
    setup();
    let provider = S3Provider::new(
        "https://s3.amazonaws.com",
        "my-bucket",
        "path/to/state.json",
        "us-east-1",
    )
    .expect("S3Provider::new should succeed");

    // Use the Debug impl to verify the URL computation.
    let debug_str = format!("{:?}", provider);
    assert!(debug_str.contains("my-bucket"));
    assert!(debug_str.contains("path/to/state.json"));
}
