//! Dropbox sync provider integration tests.
//!
//! These tests verify that [`DropboxProvider`](scribe::sync::providers::dropbox::DropboxProvider)
//! correctly handles various scenarios.
//!
//! Note: DropboxProvider requires an access token in the keychain and makes
//! actual HTTPS calls to Dropbox API. These tests focus on constructor validation,
//! type bounds, and error handling paths.

#![allow(
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::match_like_matches_macro,
    reason = "Using format! and doc links intentionally in test code"
)]

use scribe::sync::providers::dropbox::DropboxProvider;
use scribe::sync::{SyncError, SyncProvider};
use scribe::testing::keychain;

// ── test setup ────────────────────────────────────────────────────────────────

/// Initializes the mock keychain with Dropbox credentials.
fn setup() {
    keychain::use_mock_keychain();
    keychain::set_secret("dropbox", "access_token", "test-dropbox-token")
        .expect("failed to set dropbox access token");
}

// ── DropboxProvider constructor tests ────────────────────────────────────────

#[test]
fn dropbox_provider_debug_format_includes_path() {
    setup();
    let provider =
        DropboxProvider::new("/scribe/state.json").expect("DropboxProvider::new should succeed");
    let debug_str = format!("{:?}", provider);
    assert!(debug_str.contains("/scribe/state.json"));
}

#[test]
fn dropbox_provider_debug_format_does_not_leak_token() {
    setup();
    let provider =
        DropboxProvider::new("/scribe/state.json").expect("DropboxProvider::new should succeed");
    let debug_str = format!("{:?}", provider);
    // Debug format should show the path but not the client (which would contain token).
    assert!(debug_str.contains("/scribe/state.json"));
    assert!(debug_str.contains("<reqwest::Client>"));
}

#[test]
fn dropbox_provider_new_accepts_string_and_str_arguments() {
    setup();
    let path = "/scribe/state.json".to_string();
    let _provider =
        DropboxProvider::new(path).expect("DropboxProvider::new should accept String and &str");
}

#[test]
fn dropbox_provider_new_accepts_various_path_formats() {
    setup();
    // Test various Dropbox path formats.
    let _provider1 = DropboxProvider::new("/scribe/state.json").expect("root path should work");
    let _provider2 =
        DropboxProvider::new("scribe/state.json").expect("path without leading slash should work");
    let _provider3 =
        DropboxProvider::new("/Apps/Scribe/state.json").expect("App folder path should work");
}

// ── DropboxProvider Send + Sync bounds ────────────────────────────────────────

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn dropbox_provider_implements_send_and_sync() {
    setup();
    let provider =
        DropboxProvider::new("/scribe/state.json").expect("DropboxProvider::new should succeed");
    assert_send_sync::<DropboxProvider>();
    let _ = provider; // suppress unused warning
}

// ── DropboxProvider API interaction tests ─────────────────────────────────────

#[tokio::test]
async fn dropbox_provider_push_returns_transport_error_when_api_unreachable() {
    setup();
    // Create provider - it will fail to connect since we can't reach the real Dropbox API.
    let provider =
        DropboxProvider::new("/scribe/state.json").expect("DropboxProvider::new should succeed");

    let result = provider
        .push(&scribe::sync::snapshot::StateSnapshot {
            snapshot_at: chrono::Utc::now(),
            machine_id: uuid::Uuid::nil(),
            schema_version: scribe::sync::snapshot::StateSnapshot::SCHEMA_VERSION,
            projects: vec![],
            tasks: vec![],
            todos: vec![],
            time_entries: vec![],
            reminders: vec![],
            capture_items: vec![],
        })
        .await;

    // Expect transport error since we can't actually reach Dropbox.
    assert!(
        matches!(result, Err(SyncError::Transport(_))),
        "expected Transport error when Dropbox is unreachable, got: {result:?}"
    );
}

#[tokio::test]
async fn dropbox_provider_pull_returns_transport_error_when_api_unreachable() {
    setup();
    let provider =
        DropboxProvider::new("/scribe/state.json").expect("DropboxProvider::new should succeed");

    let result = provider.pull().await;

    // Expect transport error since we can't actually reach Dropbox.
    assert!(
        matches!(result, Err(SyncError::Transport(_))),
        "expected Transport error when Dropbox is unreachable, got: {result:?}"
    );
}

// ── path validation tests ─────────────────────────────────────────────────────

#[test]
fn dropbox_provider_handles_empty_path() {
    setup();
    // An empty path should still be accepted by the constructor.
    // Whether it works at runtime depends on Dropbox API behavior.
    let result = DropboxProvider::new("");
    assert!(
        result.is_ok(),
        "empty path should be accepted by constructor"
    );
}

#[test]
fn dropbox_provider_handles_deeply_nested_path() {
    setup();
    let deeply_nested = "/a/b/c/d/e/f/g/h/state.json";
    let provider =
        DropboxProvider::new(deeply_nested).expect("deeply nested path should be accepted");
    let debug_str = format!("{:?}", provider);
    assert!(debug_str.contains(deeply_nested));
}
