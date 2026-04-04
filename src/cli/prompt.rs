//! Readline-powered interactive prompts with tab completion for slug values.
//!
//! This module replaces the plain `stdin().read_line` helper used throughout
//! the CLI with a [`rustyline`]-backed prompt that provides:
//!
//! - Arrow-key editing and readline keybindings.
//! - Tab completion that queries the local `SQLite` database.
//! - In-memory history (not persisted to disk).
//! - Graceful fallback to plain `stdin` when the process is not attached to a
//!   TTY (e.g. in automated tests that pipe input).
//!
//! # Completion behaviour
//!
//! Each `prompt_*_slug` function installs a [`SlugCompleter`] that runs a
//! targeted SQL query against the shared database connection. Completing a
//! partial slug (e.g. `pay`) returns every slug that begins with `pay`, along
//! with a short hint label (name, title, or body excerpt) that rustyline
//! displays alongside the candidate.
//!
//! # Non-TTY fallback
//!
//! When stdin is not a TTY, every `prompt_*` function falls back to
//! `std::io::stdin().read_line()`, which allows test harnesses and pipes to
//! drive the prompts without blocking or erroring.

use std::io::IsTerminal as _;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::MatchingBracketHighlighter;
use rustyline::hint::HistoryHinter;
use rustyline::history::MemHistory;
use rustyline::validate::MatchingBracketValidator;
use rustyline::{Completer, Helper, Highlighter, Hinter, Validator};
use rustyline::{CompletionType, Config, Context, Editor};

// SQL query functions and SlugCandidate live in a separate file to keep this
// module within the 400-line guideline limit.
#[path = "prompt_queries.rs"]
mod queries;

use queries::SlugCandidate;

// ── SlugCompleter ──────────────────────────────────────────────────────────

/// Rustyline [`Completer`] that fetches slug candidates from `SQLite`.
///
/// Holds an `Arc<Mutex<Connection>>` and a bare function pointer so that it
/// stays generic across entity types (projects, tasks, todos, etc.) without
/// needing heap-allocated trait objects per-call.
struct SlugCompleter {
    conn: Arc<Mutex<Connection>>,
    /// Entity-specific query function.  Receives the locked connection and the
    /// prefix typed so far; returns matching candidates.
    query: fn(&Connection, &str) -> Vec<SlugCandidate>,
}

impl Completer for SlugCompleter {
    type Candidate = SlugCandidate;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<SlugCandidate>)> {
        // Slug prompts are single-token.  Treat the whole line up to the cursor
        // as the prefix so Tab always works, even if the user typed a space.
        let prefix = &line[..pos];
        let candidates = match self.conn.lock() {
            Ok(guard) => (self.query)(&guard, prefix),
            Err(_) => Vec::new(),
        };
        Ok((0, candidates))
    }
}

// ── Helper structs (rustyline derive macro requirements) ───────────────────

/// Rustyline helper that bundles a [`SlugCompleter`] with no-op traits.
#[derive(Helper, Completer, Highlighter, Hinter, Validator)]
struct SlugHelper {
    #[rustyline(Completer)]
    completer: SlugCompleter,
    #[rustyline(Highlighter)]
    highlighter: MatchingBracketHighlighter,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
    #[rustyline(Validator)]
    validator: MatchingBracketValidator,
}

/// Minimal rustyline helper used for free-text prompts with no completion.
#[derive(Helper, Completer, Highlighter, Hinter, Validator)]
struct PlainHelper {
    #[rustyline(Highlighter)]
    highlighter: MatchingBracketHighlighter,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
    #[rustyline(Validator)]
    validator: MatchingBracketValidator,
}

// ── rustyline Config (shared) ──────────────────────────────────────────────

/// Builds the rustyline [`Config`] used by all prompt functions.
// DOCUMENTED-MAGIC: CompletionType::List shows all candidates at once rather
// than cycling through them one-by-one (::Circular).  Since slug lists are
// short and users benefit from seeing all options, List is friendlier here.
fn rl_config() -> Config {
    Config::builder()
        .completion_type(CompletionType::List)
        .build()
}

// ── Plain stdin fallback ───────────────────────────────────────────────────

/// Reads one trimmed line from stdin without any readline editing.
///
/// Used when stdin is not a TTY (tests, piped input).
fn read_plain(msg: &str) -> anyhow::Result<String> {
    use std::io::Write as _;
    print!("{msg}");
    std::io::stdout()
        .flush()
        .map_err(|e| anyhow::anyhow!("stdout flush failed: {e}"))?;
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| anyhow::anyhow!("stdin read failed: {e}"))?;
    Ok(line.trim().to_owned())
}

// ── Readline wrapper ───────────────────────────────────────────────────────

/// Calls `rl.readline(msg)` and maps the result into `anyhow::Result<String>`.
///
/// EOF / Ctrl-D on an empty line returns an empty [`String`], matching the
/// original `prompt_line` behaviour (callers handle empty input as "use default").
fn readline<H>(rl: &mut Editor<H, MemHistory>, msg: &str) -> anyhow::Result<String>
where
    H: rustyline::Helper,
{
    match rl.readline(msg) {
        Ok(line) => Ok(line.trim().to_owned()),
        Err(ReadlineError::Eof | ReadlineError::Interrupted) => Ok(String::new()),
        Err(e) => Err(anyhow::anyhow!("readline error: {e}")),
    }
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Prompts for free-text input with readline editing but no completion.
///
/// Returns an empty [`String`] on empty input (just Enter or EOF/Ctrl-D).
/// Falls back to plain `stdin().read_line` when stdin is not a TTY.
///
/// # Errors
///
/// Returns an error if the underlying I/O operation fails.
pub fn prompt(msg: &str) -> anyhow::Result<String> {
    if !std::io::stdin().is_terminal() {
        return read_plain(msg);
    }
    let helper = PlainHelper {
        highlighter: MatchingBracketHighlighter::new(),
        hinter: HistoryHinter::new(),
        validator: MatchingBracketValidator::new(),
    };
    let mut rl: Editor<PlainHelper, MemHistory> =
        Editor::with_history(rl_config(), MemHistory::new())
            .map_err(|e| anyhow::anyhow!("readline init failed: {e}"))?;
    rl.set_helper(Some(helper));
    readline(&mut rl, msg)
}

/// Prompts for a project slug with tab completion from the database.
///
/// Tab-completes against active (non-archived) projects, showing
/// `slug\tname` pairs in the completion menu.
///
/// Returns an empty [`String`] on empty input.
/// Falls back to plain `stdin().read_line` when stdin is not a TTY.
///
/// # Errors
///
/// Returns an error if the underlying I/O operation fails.
pub fn prompt_project_slug(msg: &str, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<String> {
    prompt_with_completer(msg, Arc::clone(conn), queries::query_projects)
}

/// Prompts for a task slug with tab completion from the database.
///
/// Tab-completes against active (non-archived) tasks, showing
/// `slug\ttitle` pairs in the completion menu.
///
/// Returns an empty [`String`] on empty input.
/// Falls back to plain `stdin().read_line` when stdin is not a TTY.
///
/// # Errors
///
/// Returns an error if the underlying I/O operation fails.
pub fn prompt_task_slug(msg: &str, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<String> {
    prompt_with_completer(msg, Arc::clone(conn), queries::query_tasks)
}

/// Prompts for a todo slug with tab completion from the database.
///
/// Tab-completes against active (non-archived, not-done) todos, showing
/// `slug\ttitle` pairs in the completion menu.
///
/// Returns an empty [`String`] on empty input.
/// Falls back to plain `stdin().read_line` when stdin is not a TTY.
///
/// # Errors
///
/// Returns an error if the underlying I/O operation fails.
pub fn prompt_todo_slug(msg: &str, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<String> {
    prompt_with_completer(msg, Arc::clone(conn), queries::query_todos)
}

/// Prompts for a reminder slug with tab completion from the database.
///
/// Tab-completes against active (non-archived, non-fired) reminders, showing
/// `slug\tmessage_or_remind_at` pairs in the completion menu.
///
/// Returns an empty [`String`] on empty input.
/// Falls back to plain `stdin().read_line` when stdin is not a TTY.
///
/// # Errors
///
/// Returns an error if the underlying I/O operation fails.
pub fn prompt_reminder_slug(msg: &str, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<String> {
    prompt_with_completer(msg, Arc::clone(conn), queries::query_reminders)
}

/// Prompts for a capture item slug with tab completion from the database.
///
/// Tab-completes against unprocessed capture items, showing
/// `slug\tbody_excerpt` pairs in the completion menu.
///
/// Returns an empty [`String`] on empty input.
/// Falls back to plain `stdin().read_line` when stdin is not a TTY.
///
/// # Errors
///
/// Returns an error if the underlying I/O operation fails.
pub fn prompt_capture_slug(msg: &str, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<String> {
    prompt_with_completer(msg, Arc::clone(conn), queries::query_captures)
}

// ── Internal helpers ───────────────────────────────────────────────────────

/// Core implementation: builds a `SlugHelper`-backed editor and reads one line.
fn prompt_with_completer(
    msg: &str,
    conn: Arc<Mutex<Connection>>,
    query: fn(&Connection, &str) -> Vec<SlugCandidate>,
) -> anyhow::Result<String> {
    if !std::io::stdin().is_terminal() {
        return read_plain(msg);
    }
    let helper = SlugHelper {
        completer: SlugCompleter { conn, query },
        highlighter: MatchingBracketHighlighter::new(),
        hinter: HistoryHinter::new(),
        validator: MatchingBracketValidator::new(),
    };
    let mut rl: Editor<SlugHelper, MemHistory> =
        Editor::with_history(rl_config(), MemHistory::new())
            .map_err(|e| anyhow::anyhow!("readline init failed: {e}"))?;
    rl.set_helper(Some(helper));
    readline(&mut rl, msg)
}
