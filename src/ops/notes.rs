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
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use crate::domain::{NewNote, Note, NotePatch, Notes};
use crate::store::SqliteNotes;

/// High-level note operations with `$EDITOR` integration.
///
/// Construct via [`NotesOps::new`], passing the shared `SqliteNotes` store.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::{Arc, Mutex};
/// # use scribe::store::SqliteNotes;
/// # use scribe::ops::NotesOps;
/// # use scribe::db::open_in_memory;
/// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
/// let store = SqliteNotes::new(conn);
/// let ops = NotesOps::new(Arc::new(store));
/// ```
#[derive(Clone, Debug)]
pub struct NotesOps {
    notes: Arc<SqliteNotes>,
}

impl NotesOps {
    /// Creates a new [`NotesOps`] backed by the given `SqliteNotes` store.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use scribe::store::SqliteNotes;
    /// # use scribe::ops::NotesOps;
    /// # use scribe::db::open_in_memory;
    /// let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
    /// let store = SqliteNotes::new(conn);
    /// let ops = NotesOps::new(Arc::new(store));
    /// ```
    #[must_use]
    pub fn new(notes: Arc<SqliteNotes>) -> Self {
        Self { notes }
    }

    /// Returns the editor to use, preferring `EDITOR` env var then `vim`.
    fn editor() -> String {
        std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_owned())
    }

    /// Opens an existing note in `$EDITOR` and persists any changes.
    ///
    /// The current content is written to a temporary file, the editor is
    /// launched, and upon exit the file is read back and saved to the database.
    /// The temp file is deleted regardless of outcome.
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

        let edited = Self::edit_content(&note.content, &note.title)?;

        self.notes.update(
            slug,
            NotePatch {
                title: None,
                content: Some(edited),
            },
        )
    }

    /// Creates a new note with the given `slug` and immediately opens it in
    /// `$EDITOR` for content entry.
    ///
    /// If the user saves and quits the editor with empty content, the note is
    /// still created with an empty body. The temp file is deleted regardless
    /// of outcome.
    ///
    /// # Errors
    ///
    /// Returns an error if a note with that `slug` already exists, the editor
    /// fails to start, or a database error occurs during creation.
    pub fn create_and_edit(&self, title: &str, slug: &str) -> anyhow::Result<Note> {
        // Create with empty content; editor will fill it in.
        self.notes.create(NewNote {
            slug: slug.to_owned(),
            title: title.to_owned(),
            content: String::new(),
        })?;

        let edited = Self::edit_content("", title)?;

        self.notes.update(
            slug,
            NotePatch {
                title: None,
                content: Some(edited),
            },
        )
    }

    /// Launches `$EDITOR` with `content` as the initial file body and returns
    /// the text saved by the user upon editor exit.
    ///
    /// Blocks until the editor process exits. The temp file is deleted after
    /// reading regardless of outcome.
    fn edit_content(initial_content: &str, title: &str) -> anyhow::Result<String> {
        let editor = Self::editor();

        // Build a descriptive temp file name from the title, placed in /tmp/.
        let slug_part = title
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();
        let tmp_path = PathBuf::from("/tmp").join(format!("scribe-note-{slug_part}.md"));

        fs::write(&tmp_path, initial_content)
            .map_err(|e| anyhow::anyhow!("failed to write to temp file: {e}"))?;

        // Spawn editor and wait for it to exit.
        let status = Command::new(&editor)
            .arg(tmp_path.as_os_str())
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn editor '{editor}': {e}"))?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "editor '{editor}' exited with code {status}"
            ));
        }

        // Read back whatever the user saved.
        let content = fs::read_to_string(&tmp_path)
            .map_err(|e| anyhow::anyhow!("failed to read temp file after editing: {e}"))?;

        // Delete the temp file now that we've read it.
        fs::remove_file(&tmp_path)
            .map_err(|e| anyhow::anyhow!("failed to delete temp file: {e}"))?;

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
}

// ── test helpers ─────────────────────────────────────────────────────────

pub mod testing {
    //! Test helpers for the notes ops module.
    //!
    //! Re-exports internals so external integration tests can construct
    //! [`super::NotesOps`] instances against an in-memory database.

    use super::{Arc, NotesOps};
    use crate::store::note_store::testing::notes_store as make_store;

    /// Constructs a [`NotesOps`] backed by an in-memory database.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory database cannot be opened.
    #[must_use]
    pub fn ops() -> NotesOps {
        NotesOps::new(Arc::new(make_store()))
    }
}
