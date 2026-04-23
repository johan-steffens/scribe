//! `Note` entity and the `Notes` repository trait.
//!
//! Notes are markdown documents with a user-provided kebab-case slug.
//! They are the core building block of the Personal Knowledge Management layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::NoteId;

// ── entity struct ──────────────────────────────────────────────────────────

/// A note record as stored in the database.
///
/// Notes store raw markdown content and carry a user-chosen slug that serves
/// as the primary lookup key (not auto-generated like task slugs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Internal numeric primary key (not exposed to users).
    pub id: NoteId,
    /// Unique kebab-case slug chosen by the user, e.g. `arch-draft-2026`.
    pub slug: String,
    /// Short title for display purposes.
    pub title: String,
    /// Raw markdown body.
    pub content: String,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Last-updated timestamp (UTC).
    pub updated_at: DateTime<Utc>,
}

// ── repository trait ───────────────────────────────────────────────────────

/// Data-access operations for the `notes` table.
pub trait Notes {
    /// Inserts a new note and returns the persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug already exists or a database error occurs.
    fn create(&self, note: NewNote) -> anyhow::Result<Note>;

    /// Looks up a note by its slug.
    ///
    /// Returns `Ok(None)` when no note with that slug exists.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Note>>;

    /// Lists all notes, ordered by creation time.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list(&self) -> anyhow::Result<Vec<Note>>;

    /// Updates mutable fields of an existing note.
    ///
    /// # Errors
    ///
    /// Returns an error if the note does not exist or a database error occurs.
    fn update(&self, slug: &str, patch: NotePatch) -> anyhow::Result<Note>;

    /// Permanently deletes the note row from the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the note does not exist or a database error occurs.
    fn delete(&self, slug: &str) -> anyhow::Result<()>;
}

// ── input types ─────────────────────────────────────────────────────────────

/// Parameters required to create a new note.
#[derive(Debug, Clone)]
pub struct NewNote {
    /// Pre-generated unique kebab-case slug.
    pub slug: String,
    /// Short title.
    pub title: String,
    /// Raw markdown content.
    pub content: String,
}

/// Partial update for mutable note fields.
///
/// `None` values are not written.
#[derive(Debug, Clone, Default)]
pub struct NotePatch {
    /// New title, if changing.
    pub title: Option<String>,
    /// New content (markdown body), if changing.
    pub content: Option<String>,
}

// ── link type ───────────────────────────────────────────────────────────────

/// A directed link from one note/slug to another.
///
/// Links are bidirectional in concept — if note A references note B,
/// the link is stored as `source_slug = A, target_slug = B` and
/// enables efficient backlink queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// Internal numeric primary key.
    pub id: i64,
    /// The slug of the note containing the reference.
    pub source_slug: String,
    /// The slug being referenced.
    pub target_slug: String,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
}

/// Data-access operations for the `links` table.
pub trait Links {
    /// Inserts a new link, deduplicating identical source/target pairs.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn create(&self, source_slug: &str, target_slug: &str) -> anyhow::Result<Link>;

    /// Returns all links where the given slug is the source (outbound links).
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn outbound_for(&self, slug: &str) -> anyhow::Result<Vec<Link>>;

    /// Returns all links where the given slug is the target (inbound backlinks).
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn inbound_for(&self, slug: &str) -> anyhow::Result<Vec<Link>>;

    /// Removes a specific link.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn delete(&self, source_slug: &str, target_slug: &str) -> anyhow::Result<()>;

    /// Removes all links for a given note (used when deleting a note).
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn delete_all_for(&self, slug: &str) -> anyhow::Result<()>;
}
