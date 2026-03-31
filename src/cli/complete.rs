// Rust guideline compliant 2026-02-21
//! Handlers for `scribe completions` (static) and `scribe __complete` (dynamic).
//!
//! ## Two-layer completion architecture
//!
//! **Layer 1 — static** (`scribe completions <shell>`): delegates to
//! [`clap_complete::generate`] to print a full completion script for the
//! requested shell, then appends [`ZSH_DYNAMIC_SNIPPET`] or
//! [`FISH_DYNAMIC_SNIPPET`] so that slug-valued arguments are resolved
//! through live DB queries.
//!
//! **Layer 2 — dynamic** (`scribe __complete <entity>`): opens the configured
//! database and prints `<slug>\t<hint>` pairs to stdout, one per line.
//! The shell completion scripts call this subcommand to populate candidate
//! lists for positional slug arguments and `--project`/`--task` flags.
//!
//! `__complete` is intercepted from raw OS arguments in `main::run` before
//! clap parses the command line, so it does not appear in `scribe --help` and
//! does not interfere with `clap_complete`'s bash generator.

use std::io;

use clap::CommandFactory;
use clap_complete::Shell;
use rusqlite::Connection;

use crate::cli::Cli;
use crate::config::Config;
use crate::db;

// ── shell snippet constants ────────────────────────────────────────────────

/// Dynamic completion snippet appended to the zsh static script.
///
/// Defines `_scribe_dynamic_complete()` and hooks it into the generated
/// `_scribe()` completion function for every slug-valued argument.
// DOCUMENTED-MAGIC: The function name `_scribe_dynamic_complete` must be
// globally unique in the user's zsh environment; prefixing with `_scribe`
// follows the zsh convention for private completion helpers.
pub const ZSH_DYNAMIC_SNIPPET: &str = r#"
# ── scribe dynamic completions ──────────────────────────────────────────────
# Queries the live database for slug candidates with display hints.
# Called by the argument-specific completion hooks below.
_scribe_dynamic_complete() {
  local entity=$1
  local -a candidates
  local line
  while IFS=$'\t' read -r slug hint; do
    candidates+=("${slug}:${hint}")
  done < <(scribe __complete "$entity" 2>/dev/null)
  _describe 'slug' candidates
}

# ── per-argument hooks ───────────────────────────────────────────────────────

# --project <slug>
_scribe_complete_project_flag() { _scribe_dynamic_complete projects }

# --task <slug>
_scribe_complete_task_flag() { _scribe_dynamic_complete tasks }

# Positional slug for task subcommands that operate on an existing task
_scribe_complete_task_slug() { _scribe_dynamic_complete tasks }

# Positional slug for todo subcommands
_scribe_complete_todo_slug() { _scribe_dynamic_complete todos }

# Positional slug for reminder subcommands
_scribe_complete_reminder_slug() { _scribe_dynamic_complete reminders }

# Positional slug for inbox process
_scribe_complete_capture_slug() { _scribe_dynamic_complete captures }

# Patch the generated _arguments calls to use the above helpers.
# This function is called after _scribe() is defined.
_scribe_patch_completions() {
  # Override specific subcommand completion functions if they exist.
  # The generated names follow the pattern _scribe__<subcommand>__<sub>.

  # task done|edit|show|move|archive|restore|delete <slug>
  for _sub in done edit show move archive restore delete; do
    eval "_scribe__task__${_sub}() {
      local -a _args
      _arguments ':slug:_scribe_complete_task_slug' && return
    }" 2>/dev/null || true
  done

  # todo done|edit|show|move|archive|restore|delete <slug>
  for _sub in done edit show move archive restore delete; do
    eval "_scribe__todo__${_sub}() {
      local -a _args
      _arguments ':slug:_scribe_complete_todo_slug' && return
    }" 2>/dev/null || true
  done

  # reminder show|archive|restore|delete <slug>
  for _sub in show archive restore delete; do
    eval "_scribe__reminder__${_sub}() {
      local -a _args
      _arguments ':slug:_scribe_complete_reminder_slug' && return
    }" 2>/dev/null || true
  done

  # inbox process <slug>
  _scribe__inbox__process() {
    _arguments ':slug:_scribe_complete_capture_slug' && return
  }
}

# Run the patcher once the autoload machinery sources this file.
_scribe_patch_completions
"#;

/// Dynamic completion directives appended to the fish static script.
///
/// Each `complete` directive calls `scribe __complete <entity>` to source
/// live slug+hint pairs for the relevant argument position.
// DOCUMENTED-MAGIC: `complete -f` suppresses file completions for those
// arguments; `-n` guards ensure the directive fires only in the correct
// subcommand context.
pub const FISH_DYNAMIC_SNIPPET: &str = r#"
# ── scribe dynamic completions ──────────────────────────────────────────────

# Helper: emit slug<TAB>hint pairs from the __complete subcommand.
function __scribe_complete
  scribe __complete $argv[1] 2>/dev/null
end

# --project <slug>  (any subcommand)
complete -c scribe -f -l project \
  -d 'Project slug' \
  -a '(__scribe_complete projects)'

# --task <slug>  (track start)
complete -c scribe -f -l task \
  -n '__fish_seen_subcommand_from start' \
  -d 'Task slug' \
  -a '(__scribe_complete tasks)'

# task <sub> <slug>
for sub in done edit show move archive restore delete
  complete -c scribe -f \
    -n "__fish_seen_subcommand_from task; and __fish_seen_subcommand_from $sub" \
    -d 'Task slug' \
    -a '(__scribe_complete tasks)'
end

# todo <sub> <slug>
for sub in done edit show move archive restore delete
  complete -c scribe -f \
    -n "__fish_seen_subcommand_from todo; and __fish_seen_subcommand_from $sub" \
    -d 'Todo slug' \
    -a '(__scribe_complete todos)'
end

# reminder <sub> <slug>
for sub in show archive restore delete
  complete -c scribe -f \
    -n "__fish_seen_subcommand_from reminder; and __fish_seen_subcommand_from $sub" \
    -d 'Reminder slug' \
    -a '(__scribe_complete reminders)'
end

# inbox process <slug>
complete -c scribe -f \
  -n '__fish_seen_subcommand_from inbox; and __fish_seen_subcommand_from process' \
  -d 'Capture slug' \
  -a '(__scribe_complete captures)'
"#;

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
/// For zsh and fish, the dynamic snippet is appended after the static script.
/// For bash, only the static script is printed (dynamic slug completion for
/// bash would require substantially more complex shell scripting and is not
/// currently supported).
///
/// # Panics
///
/// Panics if stdout cannot be written; this mirrors the behaviour of
/// [`clap_complete::generate`].
pub fn run_completions(shell: CompletionShell) {
    // `__complete` is intercepted in main.rs before clap parsing and is
    // therefore absent from the `Cli` command tree — no filtering is needed
    // here, and all shells including bash work correctly.
    let mut cmd = Cli::command();
    let mut out = io::stdout();
    clap_complete::generate(Shell::from(shell), &mut cmd, "scribe", &mut out);

    // Append the dynamic snippet for zsh and fish.
    // Bash's completion API makes dynamic value patching significantly more
    // complex and is not supported in this release.
    match shell {
        CompletionShell::Zsh => print!("{ZSH_DYNAMIC_SNIPPET}"),
        CompletionShell::Fish => print!("{FISH_DYNAMIC_SNIPPET}"),
        CompletionShell::Bash | CompletionShell::Elvish | CompletionShell::Powershell => {}
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
