// Rust guideline compliant 2026-02-21
//! Handlers for `scribe completions` (static) and `scribe __complete` (dynamic).
//!
//! ## Two-layer completion architecture
//!
//! **Layer 1 — static** (`scribe completions <shell>`): for zsh and fish,
//! prints a fully hand-authored completion script ([`ZSH_COMPLETION`] and
//! [`FISH_COMPLETION`]) that hardcodes all subcommands, flags, enum values,
//! and calls `scribe __complete <entity>` for every slug-valued argument.
//! For bash, elvish, and powershell, delegates to [`clap_complete::generate`].
//!
//! **Layer 2 — dynamic** (`scribe __complete <entity>`): opens the configured
//! database and prints `<slug>\t<hint>` pairs to stdout, one per line.
//! The shell completion scripts call this subcommand to populate candidate
//! lists for positional slug arguments and `--project`/`--task` flags.
//!
//! `__complete` is intercepted from raw OS arguments in `main::run` before
//! clap parses the command line, so it does not appear in `scribe --help` and
//! does not interfere with `clap_complete`'s bash generator.

mod fish;
mod zsh;

pub use fish::FISH_COMPLETION;
pub use zsh::ZSH_COMPLETION;

use std::io;

use clap::CommandFactory;
use clap_complete::Shell;
use rusqlite::Connection;

use crate::cli::Cli;
use crate::config::Config;
use crate::db;

// ── CompletionsShell newtype ───────────────────────────────────────────────

/// Supported shell argument for `scribe completions`.
///
/// Thin newtype around [`clap_complete::Shell`] so the clap derive macro
/// picks it up as a `ValueEnum` automatically.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum CompletionShell {
    /// Bourne Again Shell.
    Bash,
    /// Z Shell.
    Zsh,
    /// Friendly Interactive Shell.
    Fish,
    /// Elvish shell.
    Elvish,
    /// `PowerShell`.
    Powershell,
}

impl From<CompletionShell> for Shell {
    fn from(s: CompletionShell) -> Self {
        match s {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::Zsh => Shell::Zsh,
            CompletionShell::Fish => Shell::Fish,
            CompletionShell::Elvish => Shell::Elvish,
            CompletionShell::Powershell => Shell::PowerShell,
        }
    }
}

// ── entity enum ───────────────────────────────────────────────────────────

/// Entities that can be queried for dynamic completion candidates.
///
/// Each variant maps to one DB table and one set of completion hints.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum CompleteEntity {
    /// Active (non-archived) projects: `slug\tname`.
    Projects,
    /// Active (non-archived) tasks: `slug\ttitle`.
    Tasks,
    /// Active, not-done, non-archived todos: `slug\ttitle`.
    Todos,
    /// Active (non-archived, non-fired) reminders: `slug\tmessage_or_remind_at`.
    Reminders,
    /// Unprocessed capture items: `slug\tbody_truncated`.
    Captures,
    /// Active (non-archived) time entries: `slug\tproject_slug started_at`.
    Entries,
}

// ── static completion handler ─────────────────────────────────────────────

/// Generates and prints the static completion script for `shell`.
///
/// For zsh and fish, prints a fully hand-authored script that includes live
/// slug completion via `scribe __complete <entity>`. For bash, elvish, and
/// powershell, delegates to [`clap_complete::generate`].
///
/// # Panics
///
/// Panics if stdout cannot be written; this mirrors the behaviour of
/// [`clap_complete::generate`].
pub fn run_completions(shell: CompletionShell) {
    match shell {
        CompletionShell::Zsh => print!("{ZSH_COMPLETION}"),
        CompletionShell::Fish => print!("{FISH_COMPLETION}"),
        _ => {
            let mut cmd = Cli::command();
            let mut out = io::stdout();
            clap_complete::generate(Shell::from(shell), &mut cmd, "scribe", &mut out);
        }
    }
}

// ── dynamic completion handler ────────────────────────────────────────────

/// Queries the DB and prints `slug\thint` pairs for `entity` to stdout.
///
/// Opens the user's production DB (or `SCRIBE_TEST_DB` when set). Each line
/// is `<slug>\t<hint>`, where the hint is a short human-readable label that
/// the shell shows alongside the candidate.
///
/// # Errors
///
/// Returns an error if the DB cannot be opened or a query fails.
pub fn run_complete(entity: CompleteEntity) -> anyhow::Result<()> {
    let conn = open_completion_db()?;
    match entity {
        CompleteEntity::Projects => print_projects(&conn),
        CompleteEntity::Tasks => print_tasks(&conn),
        CompleteEntity::Todos => print_todos(&conn),
        CompleteEntity::Reminders => print_reminders(&conn),
        CompleteEntity::Captures => print_captures(&conn),
        CompleteEntity::Entries => print_entries(&conn),
    }
}

/// Opens the DB for completion queries (no Arc/Mutex needed — single-threaded).
fn open_completion_db() -> anyhow::Result<Connection> {
    let db_path = if let Ok(p) = std::env::var("SCRIBE_TEST_DB") {
        std::path::PathBuf::from(p)
    } else {
        Config::load()?.db_path()
    };
    db::open(&db_path)
}

/// Prints `slug\tname` for every active (non-archived) project.
fn print_projects(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt =
        conn.prepare("SELECT slug, name FROM projects WHERE archived_at IS NULL ORDER BY slug")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        let name: String = row.get(1)?;
        println!("{slug}\t{name}");
    }
    Ok(())
}

/// Prints `slug\ttitle` for every active (non-archived) task.
fn print_tasks(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt =
        conn.prepare("SELECT slug, title FROM tasks WHERE archived_at IS NULL ORDER BY slug")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        let title: String = row.get(1)?;
        println!("{slug}\t{title}");
    }
    Ok(())
}

/// Prints `slug\ttitle` for every active (non-archived, not-done) todo.
fn print_todos(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT slug, title FROM todos \
         WHERE archived_at IS NULL AND done = 0 ORDER BY slug",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        let title: String = row.get(1)?;
        println!("{slug}\t{title}");
    }
    Ok(())
}

/// Prints `slug\tmessage_or_remind_at` for every active, non-fired reminder.
fn print_reminders(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT slug, COALESCE(message, remind_at) FROM reminders \
         WHERE archived_at IS NULL AND fired = 0 ORDER BY slug",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        let hint: String = row.get(1)?;
        println!("{slug}\t{hint}");
    }
    Ok(())
}

/// Prints `slug\tbody_truncated` for every unprocessed capture item.
///
/// Body is truncated to 60 characters so the hint fits comfortably in the
/// completion menu.
// DOCUMENTED-MAGIC: 60 chars keeps hints inside a typical 80-column terminal
// completion menu without wrapping; changes here affect display width only.
fn print_captures(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt =
        conn.prepare("SELECT slug, body FROM capture_items WHERE processed = 0 ORDER BY slug")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        let body: String = row.get(1)?;
        // Truncate at a character boundary, not a byte boundary.
        let hint: String = body.chars().take(60).collect();
        println!("{slug}\t{hint}");
    }
    Ok(())
}

/// Prints `slug\tproject_slug started_at` for every active time entry.
fn print_entries(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT te.slug, p.slug, te.started_at \
         FROM time_entries te \
         JOIN projects p ON p.id = te.project_id \
         WHERE te.archived_at IS NULL \
         ORDER BY te.slug",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let slug: String = row.get(0)?;
        let project_slug: String = row.get(1)?;
        let started_at: String = row.get(2)?;
        println!("{slug}\t{project_slug} {started_at}");
    }
    Ok(())
}
