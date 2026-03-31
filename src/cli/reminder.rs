// Rust guideline compliant 2026-02-21
//! CLI subcommands for managing reminders (`scribe reminder …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::ReminderOps`].
//! All subcommands support `--output json` for machine-readable output.
//! The `--at` flag uses [`crate::cli::parse::parse_datetime`] for flexible
//! datetime input.

use clap::{Args, Subcommand};

use crate::cli::project::OutputFormat;
use crate::ops::{ProjectOps, ReminderOps};

// ── top-level reminder command ─────────────────────────────────────────────

/// Arguments for the `scribe reminder` subcommand group.
#[derive(Debug, Args)]
pub struct ReminderCommand {
    /// Reminder subcommand.
    #[command(subcommand)]
    pub subcommand: ReminderSubcommand,
}

/// All `scribe reminder` subcommands.
#[derive(Debug, Subcommand)]
pub enum ReminderSubcommand {
    /// Create a new reminder.
    Add(ReminderAdd),
    /// List reminders.
    List(ReminderList),
    /// Show details of a reminder.
    Show(ReminderShow),
    /// Archive a reminder.
    Archive(ReminderArchive),
    /// Restore an archived reminder.
    Restore(ReminderRestore),
    /// Delete a reminder (must be archived first).
    Delete(ReminderDelete),
}

// ── subcommand structs ─────────────────────────────────────────────────────

/// Arguments for `scribe reminder add`.
#[derive(Debug, Args)]
pub struct ReminderAdd {
    /// Owning project slug.
    #[arg(long)]
    pub project: String,
    /// When the reminder should fire (flexible datetime format).
    #[arg(long)]
    pub at: String,
    /// Optional linked task slug.
    #[arg(long)]
    pub task: Option<String>,
    /// Optional message text.
    #[arg(long)]
    pub message: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe reminder list`.
#[derive(Debug, Args)]
pub struct ReminderList {
    /// Filter by project slug.
    #[arg(long)]
    pub project: Option<String>,
    /// Include archived reminders.
    #[arg(long)]
    pub archived: bool,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe reminder show`.
#[derive(Debug, Args)]
pub struct ReminderShow {
    /// Reminder slug to show.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe reminder archive`.
#[derive(Debug, Args)]
pub struct ReminderArchive {
    /// Reminder slug to archive.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe reminder restore`.
#[derive(Debug, Args)]
pub struct ReminderRestore {
    /// Reminder slug to restore.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe reminder delete`.
#[derive(Debug, Args)]
pub struct ReminderDelete {
    /// Reminder slug to delete (must be archived first).
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes a `reminder` subcommand against the given ops layers.
///
/// `project_ops` is used to resolve project slugs for filtering.
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. reminder not found, DB error).
pub fn run(
    cmd: &ReminderCommand,
    ops: &ReminderOps,
    project_ops: &ProjectOps,
) -> anyhow::Result<()> {
    match &cmd.subcommand {
        ReminderSubcommand::Add(args) => handlers::handle_add(args, ops),
        ReminderSubcommand::List(args) => handlers::handle_list(args, ops, project_ops),
        ReminderSubcommand::Show(args) => handlers::handle_show(args, ops),
        ReminderSubcommand::Archive(args) => handlers::handle_archive(args, ops),
        ReminderSubcommand::Restore(args) => handlers::handle_restore(args, ops),
        ReminderSubcommand::Delete(args) => handlers::handle_delete(args, ops),
    }
}

// ── per-subcommand handlers ────────────────────────────────────────────────

#[path = "reminder_handlers.rs"]
mod handlers;
