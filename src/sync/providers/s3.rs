// Rust guideline compliant 2026-02-21
//! S3-compatible object storage sync provider.
//!
//! [`S3Provider`] supports AWS S3, Cloudflare R2, `MinIO`, and any S3-compatible
//! endpoint. The snapshot is stored as a single JSON object at
//! `{endpoint}/{bucket}/{key}`.
//!
//! ## Authentication
//!
//! AWS access credentials must be stored in the OS keychain:
//!
//! - Access key ID: `scribe.sync.s3.access_key_id`
//! - Secret access key: `scribe.sync.s3.secret_access_key`
//!
//! See [`crate::sync::keychain`]. Run `scribe sync configure` to store them.
//!
//! ## `SigV4` signing
//!
//! Full AWS `SigV4` request signing is not yet implemented. Requests are sent
//! unsigned, which works with public buckets and development setups such as
//! `MinIO` without authentication. Full signing will be added in a follow-up
//! task via the `aws-sigv4` crate.
//!
//! # Examples
//!
//! ```no_run
//! use scribe::sync::providers::s3::S3Provider;
//! use scribe::sync::SyncProvider as _;
//!
//! # async fn example() -> Result<(), scribe::sync::SyncError> {
//! let provider = S3Provider::new(
//!     "https://s3.amazonaws.com",
//!     "my-bucket",
//!     "scribe/state.json",
//!     "us-east-1",
//! )?;
//! let snapshot = provider.pull().await?;
//! # Ok(())
//! # }
//! ```

// DOCUMENTED-MAGIC: Provider unused until the sync engine is wired in a later task.
#![allow(dead_code, reason = "wired via engine in a later task")]

use async_trait::async_trait;

use crate::sync::keychain::KeychainStore;
use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── constants ──────────────────────────────────────────────────────────────

/// MIME type for uploaded snapshot objects.
///
/// All snapshots are JSON documents. S3 uses this value for the
/// `Content-Type` metadata on the object, which allows callers to inspect
/// the object type without downloading the body.
const DEFAULT_CONTENT_TYPE: &str = "application/json";

// ── S3Provider ─────────────────────────────────────────────────────────────

/// Syncs state via S3-compatible object storage.
///
/// Supports AWS S3, Cloudflare R2, `MinIO`, and any endpoint that speaks the
/// S3 REST API. The snapshot is stored as a single JSON object.
pub struct S3Provider {
    /// S3-compatible endpoint URL, e.g. `"https://s3.amazonaws.com"`.
    endpoint: String,
    /// Name of the S3 bucket that holds the snapshot object.
    bucket: String,
    /// Object key (path) within the bucket, e.g. `"scribe/state.json"`.
    key: String,
    /// AWS region identifier, e.g. `"us-east-1"`.
    ///
    /// Required for correct `SigV4` signing once that is implemented.
    region: String,
    /// Shared HTTP client — holds a connection pool internally.
    client: reqwest::Client,
}

impl std::fmt::Debug for S3Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Provider")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("key", &self.key)
            .field("region", &self.region)
            .field("client", &"<reqwest::Client>")
            .finish()
    }
}

impl S3Provider {
    /// Creates a new `S3Provider`.
    ///
    /// `endpoint` is the S3-compatible base URL, `bucket` is the target
    /// bucket name, `key` is the object key, and `region` is the AWS region
    /// (needed for `SigV4` signing). All parameters accept both `&str` and
    /// `String`.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] if the HTTP client cannot be built.
    pub fn new(
        endpoint: impl Into<String>,
        bucket: impl Into<String>,
        key: impl Into<String>,
        region: impl Into<String>,
    ) -> Result<Self, SyncError> {
        let client = reqwest::Client::builder()
            .user_agent(super::USER_AGENT)
            .build()
            .map_err(|e| SyncError::Transport(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            endpoint: endpoint.into(),
            bucket: bucket.into(),
            key: key.into(),
            region: region.into(),
            client,
        })
    }

    /// Returns the full URL for the snapshot object.
    fn object_url(&self) -> String {
        format!("{}/{}/{}", self.endpoint, self.bucket, self.key)
    }
}

// ── SyncProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl SyncProvider for S3Provider {
    /// Serialises `snapshot` and `PUT`s it to the S3 object URL.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the AWS credentials are not in the
    ///   keychain.
    /// - [`SyncError::Auth`] if S3 returns HTTP 403.
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<(), SyncError> {
        // Read credentials from keychain (used for future SigV4 signing).
        let _access_key_id = KeychainStore::get("s3", "access_key_id")?;
        let _secret_access_key = KeychainStore::get("s3", "secret_access_key")?;

        let body = serde_json::to_vec(snapshot)
            .map_err(|e| SyncError::Transport(format!("serialisation failed: {e}")))?;

        // TODO(sigv4): Add AWS SigV4 Authorization header here.
        // Use the `aws-sigv4` crate with `_access_key_id`, `_secret_access_key`,
        // `self.region`, service = "s3", and the request body hash.
        let response = self
            .client
            .put(self.object_url())
            .header("Content-Type", DEFAULT_CONTENT_TYPE)
            .body(body)
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(SyncError::Auth(
                "S3 returned 403 — check your credentials with `scribe sync configure`".to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!("S3 returned {status}")));
        }

        Ok(())
    }

    /// Downloads the snapshot from the S3 object URL.
    ///
    /// # Errors
    ///
    /// - [`SyncError::Keychain`] if the AWS credentials are not in the
    ///   keychain.
    /// - [`SyncError::NotFound`] if S3 returns HTTP 404.
    /// - [`SyncError::Auth`] if S3 returns HTTP 403.
    /// - [`SyncError::Transport`] on any other non-success HTTP status or
    ///   network failure.
    /// - [`SyncError::InvalidSnapshot`] if the object content cannot be
    ///   deserialised as a [`StateSnapshot`].
    async fn pull(&self) -> Result<StateSnapshot, SyncError> {
        // Read credentials from keychain (used for future SigV4 signing).
        let _access_key_id = KeychainStore::get("s3", "access_key_id")?;
        let _secret_access_key = KeychainStore::get("s3", "secret_access_key")?;

        // TODO(sigv4): Add AWS SigV4 Authorization header here.
        // Use the `aws-sigv4` crate with `_access_key_id`, `_secret_access_key`,
        // `self.region`, service = "s3", and an empty body hash.
        let response = self
            .client
            .get(self.object_url())
            .send()
            .await
            .map_err(|e| SyncError::Transport(format!("request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(SyncError::NotFound(format!(
                "S3 object '{}/{}' not found",
                self.bucket, self.key
            )));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(SyncError::Auth(
                "S3 returned 403 — check your credentials with `scribe sync configure`".to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(SyncError::Transport(format!("S3 returned {status}")));
        }

        response
            .json()
            .await
            .map_err(|e| SyncError::InvalidSnapshot(format!("could not parse snapshot: {e}")))
    }
}
