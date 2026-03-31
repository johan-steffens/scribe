// Rust guideline compliant 2026-02-21
//! CLI subcommands for managing tasks (`scribe task …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::TaskOps`] or
//! [`crate::ops::ProjectOps`]. All subcommands support `--output json`.

use clap::{Args, Subcommand};
use serde_json::json;

use crate::cli::project::OutputFormat;
use crate::domain::{TaskPatch, TaskPriority, TaskStatus};
use crate::ops::tasks::CreateTask;
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

fn parse_due_date(s: &str) -> anyhow::Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("invalid date '{s}': {e}"))
}

fn handle_add(args: &TaskAdd, task_ops: &TaskOps, project_ops: &ProjectOps) -> anyhow::Result<()> {
    let project_slug = args
        .project
        .clone()
        .unwrap_or_else(|| "quick-capture".to_owned());
    let project = project_ops
        .get_project(&project_slug)?
        .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;

    let due_date = args.due.as_deref().map(parse_due_date).transpose()?;

    let task = task_ops.create_task(CreateTask {
        project_slug: project.slug.clone(),
        project_id: project.id,
        title: args.title.clone(),
        description: None,
        status: TaskStatus::Todo,
        priority: args.priority,
        due_date,
    })?;

    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Created task: {} [{}]", task.title, task.slug),
    }
    Ok(())
}

fn handle_list(
    args: &TaskList,
    task_ops: &TaskOps,
    project_ops: &ProjectOps,
) -> anyhow::Result<()> {
    let project_id = args
        .project
        .as_deref()
        .map(|slug| {
            project_ops
                .get_project(slug)?
                .ok_or_else(|| anyhow::anyhow!("project '{slug}' not found"))
                .map(|p| p.id)
        })
        .transpose()?;

    let tasks = task_ops.list_tasks(project_id, args.status, args.priority, args.archived)?;

    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&tasks)?),
        OutputFormat::Text => {
            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                for t in &tasks {
                    let archived = if t.archived_at.is_some() {
                        " [archived]"
                    } else {
                        ""
                    };
                    println!("{:<45} [{}] [{}]{}", t.slug, t.status, t.priority, archived);
                }
            }
        }
    }
    Ok(())
}

fn handle_show(args: &TaskShow, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops
        .get_task(&args.slug)?
        .ok_or_else(|| anyhow::anyhow!("task '{}' not found", args.slug))?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => {
            println!("slug:        {}", task.slug);
            println!("title:       {}", task.title);
            println!("status:      {}", task.status);
            println!("priority:    {}", task.priority);
            println!(
                "due:         {}",
                task.due_date
                    .map(|d| d.to_string())
                    .as_deref()
                    .unwrap_or("—")
            );
            println!(
                "description: {}",
                task.description.as_deref().unwrap_or("—")
            );
            println!(
                "created:     {}",
                task.created_at.format("%Y-%m-%d %H:%M UTC")
            );
        }
    }
    Ok(())
}

fn handle_edit(args: &TaskEdit, task_ops: &TaskOps) -> anyhow::Result<()> {
    let due_date = args.due.as_deref().map(parse_due_date).transpose()?;

    let patch = TaskPatch {
        title: args.title.clone(),
        description: None,
        clear_description: false,
        status: args.status,
        priority: args.priority,
        due_date,
        clear_due_date: false,
        project_id: None,
    };
    let task = task_ops.update_task(&args.slug, patch)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Updated task: {} [{}]", task.title, task.slug),
    }
    Ok(())
}

fn handle_move(
    args: &TaskMove,
    task_ops: &TaskOps,
    project_ops: &ProjectOps,
) -> anyhow::Result<()> {
    let project = project_ops
        .get_project(&args.project)?
        .ok_or_else(|| anyhow::anyhow!("project '{}' not found", args.project))?;
    let task = task_ops.update_task(
        &args.slug,
        TaskPatch {
            project_id: Some(project.id),
            ..Default::default()
        },
    )?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Moved task '{}' to project '{}'", task.slug, args.project),
    }
    Ok(())
}

fn handle_done(args: &TaskDone, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops.mark_done(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Done: {}", task.slug),
    }
    Ok(())
}

fn handle_archive(args: &TaskArchive, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops.archive_task(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Archived task: {}", task.slug),
    }
    Ok(())
}

fn handle_restore(args: &TaskRestore, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops.restore_task(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Restored task: {}", task.slug),
    }
    Ok(())
}

fn handle_delete(args: &TaskDelete, task_ops: &TaskOps) -> anyhow::Result<()> {
    task_ops.delete_task(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", json!({ "deleted": args.slug })),
        OutputFormat::Text => println!("Deleted task: {}", args.slug),
    }
    Ok(())
}
