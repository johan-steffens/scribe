//! SQL query helpers used by `SlugCompleter` in [`super::prompt`].
//!
//! Each function receives a locked [`rusqlite::Connection`] and a slug prefix,
//! and returns a `Vec<SlugCandidate>` for tab-completion menus.  Any database
//! error is silently swallowed and an empty list is returned so that a
//! transient error never crashes the interactive prompt.

use rusqlite::Connection;

// ── Completion candidate ───────────────────────────────────────────────────

/// A slug completion candidate, paired with a human-readable hint.
///
/// `display` is shown in the completion menu (slug + tab + hint).
/// `replacement` is the text actually inserted into the line buffer.
#[derive(Debug, Clone)]
pub(super) struct SlugCandidate {
    /// Text inserted into the line buffer on completion.
    pub(super) replacement: String,
    /// Text shown in the completion menu (includes hint after `\t`).
    pub(super) display: String,
}

impl rustyline::completion::Candidate for SlugCandidate {
    fn display(&self) -> &str {
        &self.display
    }

    fn replacement(&self) -> &str {
        &self.replacement
    }
}

/// Returns project slugs starting with `prefix` from the active projects table.
pub(super) fn query_projects(conn: &Connection, prefix: &str) -> Vec<SlugCandidate> {
    let pattern = format!("{prefix}%");
    let Ok(mut stmt) = conn.prepare(
        "SELECT slug, name FROM projects \
         WHERE archived_at IS NULL AND slug LIKE ?1 \
         ORDER BY slug",
    ) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map(|rows| {
        rows.filter_map(Result::ok)
            .map(|(slug, name)| SlugCandidate {
                display: format!("{slug}\t{name}"),
                replacement: slug,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Returns task slugs starting with `prefix` from the active tasks table.
pub(super) fn query_tasks(conn: &Connection, prefix: &str) -> Vec<SlugCandidate> {
    let pattern = format!("{prefix}%");
    let Ok(mut stmt) = conn.prepare(
        "SELECT slug, title FROM tasks \
         WHERE archived_at IS NULL AND slug LIKE ?1 \
         ORDER BY slug",
    ) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map(|rows| {
        rows.filter_map(Result::ok)
            .map(|(slug, title)| SlugCandidate {
                display: format!("{slug}\t{title}"),
                replacement: slug,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Returns todo slugs starting with `prefix` from active, not-done todos.
pub(super) fn query_todos(conn: &Connection, prefix: &str) -> Vec<SlugCandidate> {
    let pattern = format!("{prefix}%");
    let Ok(mut stmt) = conn.prepare(
        "SELECT slug, title FROM todos \
         WHERE archived_at IS NULL AND done = 0 AND slug LIKE ?1 \
         ORDER BY slug",
    ) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map(|rows| {
        rows.filter_map(Result::ok)
            .map(|(slug, title)| SlugCandidate {
                display: format!("{slug}\t{title}"),
                replacement: slug,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Returns reminder slugs starting with `prefix` from active, non-fired reminders.
pub(super) fn query_reminders(conn: &Connection, prefix: &str) -> Vec<SlugCandidate> {
    let pattern = format!("{prefix}%");
    let Ok(mut stmt) = conn.prepare(
        "SELECT slug, COALESCE(message, remind_at) FROM reminders \
         WHERE archived_at IS NULL AND fired = 0 AND slug LIKE ?1 \
         ORDER BY slug",
    ) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map(|rows| {
        rows.filter_map(Result::ok)
            .map(|(slug, hint)| SlugCandidate {
                display: format!("{slug}\t{hint}"),
                replacement: slug,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Returns capture item slugs starting with `prefix` from unprocessed items.
///
/// Body excerpts are truncated to 60 characters so the hint fits in a
/// typical 80-column terminal completion menu.
// DOCUMENTED-MAGIC: 60 chars matches the truncation in `complete/mod.rs`
// `print_captures`; both must stay in sync for consistent UX across shell
// completions and readline completions.
pub(super) fn query_captures(conn: &Connection, prefix: &str) -> Vec<SlugCandidate> {
    let pattern = format!("{prefix}%");
    let Ok(mut stmt) = conn.prepare(
        "SELECT slug, body FROM capture_items \
         WHERE processed = 0 AND slug LIKE ?1 \
         ORDER BY slug",
    ) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map(|rows| {
        rows.filter_map(Result::ok)
            .map(|(slug, body)| {
                let hint: String = body.chars().take(60).collect();
                SlugCandidate {
                    display: format!("{slug}\t{hint}"),
                    replacement: slug,
                }
            })
            .collect()
    })
    .unwrap_or_default()
}
