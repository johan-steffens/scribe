//! Business logic operations for the note entity.
//!
//! [`NotesOps`] wraps [`SqliteNotes`] and provides an `$EDITOR`-based editing
//! workflow. When a user wants to write or revise a note, Scribe opens the
//! current content in their preferred editor (via the `EDITOR` environment
//! variable, falling back to `vim`), waits for the user to save and quit, and
//! then persists the updated content back to the database.
//!
//! # Editor Workflow
//!
//! 1. Fetch existing note content from `SQLite` (or start with an empty string).
//! 2. Write content to a temporary file (`/tmp/scribe-note-<slug>.md`).
//! 3. Spawn `$EDITOR` targeting that temp file.
//! 4. Wait for the editor to exit.
//! 5. Read the temp file and save updated content to `SQLite`.
//! 6. Delete the temp file.

use std::fs;
use std::io::Write as _;
use std::process::Command;
use std::sync::Arc;

use crate::domain::{Links, NewNote, Note, NotePatch, Notes, parse_links};
use crate::store::{SqliteLinks, SqliteNotes};

/// High-level note operations with `$EDITOR` integration.
///
/// Construct via [`NotesOps::new`], passing the shared `SqliteNotes` and
/// `SqliteLinks` stores. The optional `note_editor` parameter overrides the
/// default editor chain (config → `$EDITOR` → `vim`).
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::store::{SqliteNotes, SqliteLinks};
/// # use scribe::ops::NotesOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let notes = SqliteNotes::new(Arc::clone(&conn));
/// let links = SqliteLinks::new(conn);
/// let ops = NotesOps::new(Arc::new(notes), Arc::new(links), None);
/// ```
#[derive(Clone, Debug)]
pub struct NotesOps {
    notes: Arc<SqliteNotes>,
    links: Arc<SqliteLinks>,
    /// Optional note editor override, falling back to `$EDITOR` env var then `vim`.
    note_editor: Option<String>,
}

impl NotesOps {
    /// Creates a new [`NotesOps`] backed by the given `SqliteNotes` and
    /// `SqliteLinks` stores.
    ///
    /// The optional `note_editor` parameter sets the preferred editor, falling
    /// back to `$EDITOR` env var then `vim` when `None`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::{SqliteNotes, SqliteLinks};
    /// # use scribe::ops::NotesOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let notes = SqliteNotes::new(Arc::clone(&conn));
    /// let links = SqliteLinks::new(conn);
    /// let ops = NotesOps::new(Arc::new(notes), Arc::new(links), None);
    /// ```
    #[must_use]
    pub fn new(
        notes: Arc<SqliteNotes>,
        links: Arc<SqliteLinks>,
        note_editor: Option<String>,
    ) -> Self {
        Self {
            notes,
            links,
            note_editor,
        }
    }

    /// Synchronizes the `links` table after a note is saved.
    ///
    /// This parses the note content for `[[slug]]` patterns, removes all
    /// existing links where the note is the source, and inserts new links
    /// for each unique slug found.
    ///
    /// # Errors
    ///
    /// Returns an error if database operations fail.
    pub fn sync_links(&self, source_slug: &str, content: &str) -> anyhow::Result<()> {
        // Parse all [[slug]] references from content.
        let target_slugs = parse_links(content);

        // Delete all existing outbound links from this source.
        self.links.delete_all_for(source_slug)?;

        // Insert new links for each unique target slug.
        // `SqliteLinks::create` uses INSERT OR IGNORE and re-reads the row, so
        // duplicates are fine and real DB failures still surface as errors.
        for target_slug in &target_slugs {
            if target_slug == source_slug {
                continue;
            }
            self.links.create(source_slug, target_slug)?;
        }
        Ok(())
    }

    /// Returns the editor to use.
    ///
    /// Uses the configured `note_editor` if set, falling back to `$EDITOR` env
    /// var, then `vim`.
    fn editor(&self) -> String {
        self.note_editor
            .clone()
            .or_else(|| std::env::var("EDITOR").ok())
            .unwrap_or_else(|| "vim".to_owned())
    }

    /// Opens an existing note in `$EDITOR` and persists any changes.
    ///
    /// The current content is written to a temporary file, the editor is
    /// launched, and upon exit the file is read back and saved to the database.
    /// The temp file is deleted regardless of outcome.
    ///
    /// After saving, [`sync_links`] is called to update the backlinks table
    /// based on `[[slug]]` patterns found in the new content.
    ///
    /// # Errors
    ///
    /// Returns an error if the note does not exist, the editor fails to start,
    /// or a database error occurs during the save.
    pub fn edit_note(&self, slug: &str) -> anyhow::Result<Note> {
        let note = self
            .notes
            .find_by_slug(slug)?
            .ok_or_else(|| anyhow::anyhow!("note '{slug}' not found"))?;

        let edited = self.edit_content(&note.content, &note.title)?;

        let updated = self.notes.update(
            slug,
            NotePatch {
                title: None,
                content: Some(edited),
            },
        )?;

        self.sync_links(slug, &updated.content)?;

        Ok(updated)
    }

    /// Creates a new note with the given `slug` and immediately opens it in
    /// `$EDITOR` for content entry.
    ///
    /// If the user saves and quits the editor with empty content, the note is
    /// still created with an empty body. The temp file is deleted regardless
    /// of outcome.
    ///
    /// After saving, [`sync_links`] is called to update the backlinks table
    /// based on `[[slug]]` patterns found in the new content.
    ///
    /// # Errors
    ///
    /// Returns an error if a note with that `slug` already exists, the editor
    /// fails to start, or a database error occurs during creation.
    pub fn create_and_edit(&self, title: &str, slug: &str) -> anyhow::Result<Note> {
        let title = title.trim();
        let slug = slug.trim();
        if title.is_empty() {
            return Err(anyhow::anyhow!("note title cannot be empty"));
        }
        if slug.is_empty() {
            return Err(anyhow::anyhow!("note slug cannot be empty"));
        }

        // Create with empty content; editor will fill it in.
        self.notes.create(NewNote {
            slug: slug.to_owned(),
            title: title.to_owned(),
            content: String::new(),
        })?;

        let edited = self.edit_content("", title)?;

        let updated = self.notes.update(
            slug,
            NotePatch {
                title: None,
                content: Some(edited),
            },
        )?;

        self.sync_links(slug, &updated.content)?;

        Ok(updated)
    }

    /// Launches `$EDITOR` with `content` as the initial file body and returns
    /// the text saved by the user upon editor exit.
    ///
    /// Blocks until the editor process exits. The temp file is deleted after
    /// reading regardless of outcome.
    fn edit_content(&self, initial_content: &str, title: &str) -> anyhow::Result<String> {
        let editor = self.editor();

        // OS temp dir via `tempfile` (portable; cleaned up on drop).
        // DOCUMENTED-MAGIC: prefix includes a short title fragment so editors
        // that show the path in the title bar remain recognisable to the user.
        let slug_part: String = title
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .take(40)
            .collect();
        let prefix = if slug_part.is_empty() {
            "scribe-note-".to_owned()
        } else {
            format!("scribe-note-{slug_part}-")
        };
        let mut tmp = tempfile::Builder::new()
            .prefix(&prefix)
            .suffix(".md")
            .tempfile()
            .map_err(|e| anyhow::anyhow!("failed to create temp file: {e}"))?;

        tmp.write_all(initial_content.as_bytes())
            .map_err(|e| anyhow::anyhow!("failed to write to temp file: {e}"))?;
        tmp.flush()
            .map_err(|e| anyhow::anyhow!("failed to flush temp file: {e}"))?;

        let tmp_path = tmp.path().to_path_buf();

        // Spawn editor and wait for it to exit.
        let status = Command::new(&editor)
            .arg(&tmp_path)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn editor '{editor}': {e}"))?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "editor '{editor}' exited with code {status}"
            ));
        }

        // Read back whatever the user saved; `tmp` drops and removes the file.
        let content = fs::read_to_string(&tmp_path)
            .map_err(|e| anyhow::anyhow!("failed to read temp file after editing: {e}"))?;

        Ok(content)
    }

    /// Returns the note with the given `slug`, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn get(&self, slug: &str) -> anyhow::Result<Option<Note>> {
        self.notes.find_by_slug(slug)
    }

    /// Lists all notes ordered by creation time.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn list(&self) -> anyhow::Result<Vec<Note>> {
        self.notes.list()
    }

    /// Creates a new note with the given title and content.
    ///
    /// The slug is auto-generated from the title. After creation,
    /// [`sync_links`] is called to update the backlinks table based on
    /// `[[slug]]` patterns found in the content.
    ///
    /// # Errors
    ///
    /// Returns an error if slug generation fails or a database error occurs.
    pub fn write_note(&self, title: &str, content: &str) -> anyhow::Result<Note> {
        use crate::domain::slug;

        let title = title.trim();
        if title.is_empty() {
            return Err(anyhow::anyhow!("note title cannot be empty"));
        }

        let slug_str = slug::generate("note-", title);
        let unique_slug = slug::ensure_unique(&slug_str, |candidate| {
            self.notes
                .find_by_slug(candidate)
                .is_ok_and(|r| r.is_some())
        })
        .map_err(|e| anyhow::anyhow!("slug generation failed: {e}"))?;

        let note = self.notes.create(NewNote {
            slug: unique_slug,
            title: title.to_owned(),
            content: content.to_owned(),
        })?;

        self.sync_links(&note.slug, &note.content)?;

        Ok(note)
    }

    /// Searches notes by full-text query.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    pub fn search_notes(&self, query: &str) -> anyhow::Result<Vec<Note>> {
        self.notes.search_notes(query)
    }

    /// Deletes the note with the given slug.
    ///
    /// # Errors
    ///
    /// Returns an error if the note does not exist or a database error occurs.
    pub fn delete_note(&self, slug: &str) -> anyhow::Result<()> {
        self.notes.delete(slug)
    }

    /// Returns a reference to the underlying links store.
    ///
    /// This is intended for use in tests that need to verify link state.
    #[cfg(feature = "test-util")]
    #[must_use]
    #[expect(
        clippy::missing_const_for_fn,
        reason = "const not needed for test helper"
    )]
    pub fn links_store(&self) -> &Arc<SqliteLinks> {
        &self.links
    }
}

// ── test helpers ─────────────────────────────────────────────────────────

pub mod testing {
    //! Test helpers for the notes ops module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::NotesOps`] instances against an in-memory database.

    use super::{Arc, NotesOps};
    use crate::store::note_store::testing::{links_store as make_links, notes_store as make_notes};

    /// Constructs a [`NotesOps`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn ops() -> NotesOps {
        NotesOps::new(Arc::new(make_notes()), Arc::new(make_links()), None)
    }
}
