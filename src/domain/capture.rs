// Rust guideline compliant 2026-02-21
//! `CaptureItem` entity and the `CaptureItems` repository trait.
//!
//! Capture items are raw inbox entries — they have no project binding by
//! design. Users triage them later via `scribe inbox process`. Slugs are
//! auto-generated from the creation time, e.g. `capture-20260331-143000`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::CaptureItemId;

// ── entity struct ──────────────────────────────────────────────────────────

/// A capture item (raw inbox entry) as stored in the database.
///
/// Unlike other entities, capture items have no `project_id` or `archived_at`
/// — they are either unprocessed or processed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureItem {
    /// Internal numeric primary key (not exposed to users).
    pub id: CaptureItemId,
    /// Unique slug, e.g. `capture-20260331-143000`.
    pub slug: String,
    /// Raw text body of the captured thought.
    pub body: String,
    /// Whether the item has been triaged and processed.
    pub processed: bool,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
}

// ── repository trait ───────────────────────────────────────────────────────

/// Data-access operations for the `capture_items` table.
// TODO(phase3): migrate to domain error structs per M-ERRORS-CANONICAL-STRUCTS
pub trait CaptureItems {
    /// Inserts a new capture item and returns the persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug already exists or a database error occurs.
    fn create(&self, item: NewCaptureItem) -> anyhow::Result<CaptureItem>;

    /// Looks up a capture item by its slug.
    ///
    /// Returns `Ok(None)` when no item with that slug exists.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<CaptureItem>>;

    /// Lists capture items.
    ///
    /// When `include_processed` is `false`, only unprocessed items are
    /// returned.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list(&self, include_processed: bool) -> anyhow::Result<Vec<CaptureItem>>;

    /// Marks the item as processed.
    ///
    /// # Errors
    ///
    /// Returns an error if the item does not exist or a database error occurs.
    fn mark_processed(&self, slug: &str) -> anyhow::Result<CaptureItem>;

    /// Permanently deletes the capture item row.
    ///
    /// # Errors
    ///
    /// Returns an error if the item does not exist or a database error occurs.
    // Used in Phase 3+ TUI hard-delete flows.
    #[expect(dead_code, reason = "used in Phase 3+ TUI hard-delete flows")]
    fn delete(&self, slug: &str) -> anyhow::Result<()>;
}

// ── input types ────────────────────────────────────────────────────────────

/// Parameters required to create a new capture item.
#[derive(Debug, Clone)]
pub struct NewCaptureItem {
    /// Pre-generated unique slug.
    pub slug: String,
    /// Raw body text.
    pub body: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}
