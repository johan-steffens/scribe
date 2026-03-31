// Rust guideline compliant 2026-02-21
//! CLI subcommands for managing tasks (`scribe task …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::TaskOps`] or
//! [`crate::ops::ProjectOps`]. All subcommands support `--output json`.

use clap::{Args, Subcommand};

use crate::cli::project::OutputFormat;
use crate::domain::{TaskPriority, TaskStatus};
use crate::ops::{ProjectOps, TaskOps};

// ── clap impls for enums ───────────────────────────────────────────────────

impl clap::ValueEnum for TaskStatus {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Todo, Self::InProgress, Self::Done, Self::Cancelled]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }))
    }
}

impl clap::ValueEnum for TaskPriority {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Low, Self::Medium, Self::High, Self::Urgent]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Urgent => "urgent",
        }))
    }
}

// ── top-level task command ─────────────────────────────────────────────────

/// Arguments for the `scribe task` subcommand group.
#[derive(Debug, Args)]
pub struct TaskCommand {
    /// Task subcommand.
    #[command(subcommand)]
    pub subcommand: TaskSubcommand,
}

/// All `scribe task` subcommands.
#[derive(Debug, Subcommand)]
pub enum TaskSubcommand {
    /// Create a new task.
    Add(TaskAdd),
    /// List tasks.
    List(TaskList),
    /// Show details of a task.
    Show(TaskShow),
    /// Edit a task's fields.
    Edit(TaskEdit),
    /// Move a task to a different project.
    Move(TaskMove),
    /// Mark a task as done.
    Done(TaskDone),
    /// Archive a task.
    Archive(TaskArchive),
    /// Restore an archived task.
    Restore(TaskRestore),
    /// Delete a task.
    Delete(TaskDelete),
}

// ── subcommand structs ─────────────────────────────────────────────────────

/// Arguments for `scribe task add`.
#[derive(Debug, Args)]
pub struct TaskAdd {
    /// Task title (slug is auto-generated).
    pub title: String,
    /// Project slug (defaults to `quick-capture`).
    #[arg(long)]
    pub project: Option<String>,
    /// Task priority.
    #[arg(long, default_value = "medium")]
    pub priority: TaskPriority,
    /// Due date in `YYYY-MM-DD` format.
    #[arg(long)]
    pub due: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task list`.
#[derive(Debug, Args)]
pub struct TaskList {
    /// Filter by project slug.
    #[arg(long)]
    pub project: Option<String>,
    /// Filter by status.
    #[arg(long)]
    pub status: Option<TaskStatus>,
    /// Filter by priority.
    #[arg(long)]
    pub priority: Option<TaskPriority>,
    /// Include archived tasks.
    #[arg(long)]
    pub archived: bool,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task show`.
#[derive(Debug, Args)]
pub struct TaskShow {
    /// Task slug to show.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task edit`.
#[derive(Debug, Args)]
pub struct TaskEdit {
    /// Task slug to edit.
    pub slug: String,
    /// New title.
    #[arg(long)]
    pub title: Option<String>,
    /// New status.
    #[arg(long)]
    pub status: Option<TaskStatus>,
    /// New priority.
    #[arg(long)]
    pub priority: Option<TaskPriority>,
    /// New due date.
    #[arg(long)]
    pub due: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task move`.
#[derive(Debug, Args)]
pub struct TaskMove {
    /// Task slug to move.
    pub slug: String,
    /// Destination project slug.
    #[arg(long)]
    pub project: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task done`.
#[derive(Debug, Args)]
pub struct TaskDone {
    /// Task slug to mark as done.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task archive`.
#[derive(Debug, Args)]
pub struct TaskArchive {
    /// Task slug to archive.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task restore`.
#[derive(Debug, Args)]
pub struct TaskRestore {
    /// Task slug to restore.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe task delete`.
#[derive(Debug, Args)]
pub struct TaskDelete {
    /// Task slug to delete.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes a `task` subcommand against the given ops layers.
///
/// `project_ops` is needed to resolve project slugs to IDs. Prints results to
/// stdout and errors to stderr. Returns `Ok(())` on success.
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. task not found, DB error).
pub fn run(cmd: &TaskCommand, task_ops: &TaskOps, project_ops: &ProjectOps) -> anyhow::Result<()> {
    match &cmd.subcommand {
        TaskSubcommand::Add(args) => handle_add(args, task_ops, project_ops),
        TaskSubcommand::List(args) => handle_list(args, task_ops, project_ops),
        TaskSubcommand::Show(args) => handle_show(args, task_ops),
        TaskSubcommand::Edit(args) => handle_edit(args, task_ops),
        TaskSubcommand::Move(args) => handle_move(args, task_ops, project_ops),
        TaskSubcommand::Done(args) => handle_done(args, task_ops),
        TaskSubcommand::Archive(args) => handle_archive(args, task_ops),
        TaskSubcommand::Restore(args) => handle_restore(args, task_ops),
        TaskSubcommand::Delete(args) => handle_delete(args, task_ops),
    }
}

// ── per-subcommand handlers ────────────────────────────────────────────────

#[path = "task_handlers.rs"]
mod handlers;

use handlers::{
    handle_add, handle_archive, handle_delete, handle_done, handle_edit, handle_list, handle_move,
    handle_restore, handle_show,
};
