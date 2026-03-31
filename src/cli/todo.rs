// Rust guideline compliant 2026-02-21
//! CLI subcommands for managing todos (`scribe todo …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::TodoOps`].
//! All subcommands support `--output json` for machine-readable output.

use clap::{Args, Subcommand};

use crate::cli::project::OutputFormat;
use crate::ops::{ProjectOps, TodoOps};

// ── top-level todo command ─────────────────────────────────────────────────

/// Arguments for the `scribe todo` subcommand group.
#[derive(Debug, Args)]
pub struct TodoCommand {
    /// Todo subcommand.
    #[command(subcommand)]
    pub subcommand: TodoSubcommand,
}

/// All `scribe todo` subcommands.
#[derive(Debug, Subcommand)]
pub enum TodoSubcommand {
    /// Create a new todo.
    Add(TodoAdd),
    /// List todos.
    List(TodoList),
    /// Show details of a todo.
    Show(TodoShow),
    /// Move a todo to a different project.
    Move(TodoMove),
    /// Mark a todo as done.
    Done(TodoDone),
    /// Archive a todo.
    Archive(TodoArchive),
    /// Restore an archived todo.
    Restore(TodoRestore),
    /// Delete a todo (must be archived first).
    Delete(TodoDelete),
}

// ── subcommand structs ─────────────────────────────────────────────────────

/// Arguments for `scribe todo add`.
#[derive(Debug, Args)]
pub struct TodoAdd {
    /// Todo title (slug is auto-generated).
    pub title: String,
    /// Project slug (defaults to `quick-capture`).
    #[arg(long)]
    pub project: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe todo list`.
#[derive(Debug, Args)]
pub struct TodoList {
    /// Filter by project slug.
    #[arg(long)]
    pub project: Option<String>,
    /// Include done todos.
    #[arg(long)]
    pub all: bool,
    /// Show archived todos instead.
    #[arg(long)]
    pub archived: bool,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe todo show`.
#[derive(Debug, Args)]
pub struct TodoShow {
    /// Todo slug to show.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe todo move`.
#[derive(Debug, Args)]
pub struct TodoMove {
    /// Todo slug to move.
    pub slug: String,
    /// Destination project slug.
    #[arg(long)]
    pub project: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe todo done`.
#[derive(Debug, Args)]
pub struct TodoDone {
    /// Todo slug to mark as done.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe todo archive`.
#[derive(Debug, Args)]
pub struct TodoArchive {
    /// Todo slug to archive.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe todo restore`.
#[derive(Debug, Args)]
pub struct TodoRestore {
    /// Todo slug to restore.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe todo delete`.
#[derive(Debug, Args)]
pub struct TodoDelete {
    /// Todo slug to delete (must be archived first).
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes a `todo` subcommand against the given ops layers.
///
/// `project_ops` is used to resolve project slugs to IDs for list filtering.
/// Prints results to stdout; errors are propagated to the caller.
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. todo not found, DB error).
pub fn run(cmd: &TodoCommand, ops: &TodoOps, project_ops: &ProjectOps) -> anyhow::Result<()> {
    match &cmd.subcommand {
        TodoSubcommand::Add(args) => handlers::handle_add(args, ops),
        TodoSubcommand::List(args) => handlers::handle_list(args, ops, project_ops),
        TodoSubcommand::Show(args) => handlers::handle_show(args, ops),
        TodoSubcommand::Move(args) => handlers::handle_move(args, ops),
        TodoSubcommand::Done(args) => handlers::handle_done(args, ops),
        TodoSubcommand::Archive(args) => handlers::handle_archive(args, ops),
        TodoSubcommand::Restore(args) => handlers::handle_restore(args, ops),
        TodoSubcommand::Delete(args) => handlers::handle_delete(args, ops),
    }
}

// ── per-subcommand handlers ────────────────────────────────────────────────

#[path = "todo_handlers.rs"]
mod handlers;
