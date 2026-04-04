//! CLI subcommands for managing projects (`scribe project …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::ProjectOps`].
//! All subcommands support `--output json` for machine-readable output.

use clap::{Args, Subcommand};
use serde_json::json;

use crate::domain::{NewProject, ProjectPatch, ProjectStatus};
use crate::ops::ProjectOps;

/// Output format for CLI commands.
#[derive(Debug, Clone, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text (default).
    #[default]
    Text,
    /// Machine-readable JSON.
    Json,
}

// ── top-level project command ──────────────────────────────────────────────

/// Arguments for the `scribe project` subcommand group.
#[derive(Debug, Args)]
pub struct ProjectCommand {
    /// Project subcommand.
    #[command(subcommand)]
    pub subcommand: ProjectSubcommand,
}

/// All `scribe project` subcommands.
#[derive(Debug, Subcommand)]
pub enum ProjectSubcommand {
    /// Create a new project.
    Add(ProjectAdd),
    /// List projects.
    List(ProjectList),
    /// Show details of a project.
    Show(ProjectShow),
    /// Edit a project's fields.
    Edit(ProjectEdit),
    /// Archive a project and all its items.
    Archive(ProjectArchive),
    /// Restore an archived project.
    Restore(ProjectRestore),
    /// Delete a project (must be empty or archived first).
    Delete(ProjectDelete),
}

// ── subcommand structs ─────────────────────────────────────────────────────

/// Arguments for `scribe project add`.
#[derive(Debug, Args)]
pub struct ProjectAdd {
    /// Unique kebab-case slug for the project.
    pub slug: String,
    /// Human-readable project name.
    #[arg(long)]
    pub name: String,
    /// Optional project description.
    #[arg(long)]
    pub desc: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe project list`.
#[derive(Debug, Args)]
pub struct ProjectList {
    /// Filter by status.
    #[arg(long)]
    pub status: Option<ProjectStatus>,
    /// Include archived projects.
    #[arg(long)]
    pub archived: bool,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe project show`.
#[derive(Debug, Args)]
pub struct ProjectShow {
    /// Project slug to show.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe project edit`.
#[derive(Debug, Args)]
pub struct ProjectEdit {
    /// Project slug to edit.
    pub slug: String,
    /// New slug.
    #[arg(long)]
    pub new_slug: Option<String>,
    /// New name.
    #[arg(long)]
    pub name: Option<String>,
    /// New description.
    #[arg(long)]
    pub desc: Option<String>,
    /// New status.
    #[arg(long)]
    pub status: Option<ProjectStatus>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe project archive`.
#[derive(Debug, Args)]
pub struct ProjectArchive {
    /// Project slug to archive.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe project restore`.
#[derive(Debug, Args)]
pub struct ProjectRestore {
    /// Project slug to restore.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe project delete`.
#[derive(Debug, Args)]
pub struct ProjectDelete {
    /// Project slug to delete.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

// ── clap impl for ProjectStatus ────────────────────────────────────────────

impl clap::ValueEnum for ProjectStatus {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Active, Self::Paused, Self::Completed]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
        }))
    }
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes a `project` subcommand against the given ops layer.
///
/// Prints results to stdout and errors to stderr. Returns `Ok(())` on
/// success or an error that the caller converts to an exit code.
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. project not found, DB error).
pub fn run(cmd: &ProjectCommand, ops: &ProjectOps) -> anyhow::Result<()> {
    match &cmd.subcommand {
        ProjectSubcommand::Add(args) => handle_add(args, ops),
        ProjectSubcommand::List(args) => handle_list(args, ops),
        ProjectSubcommand::Show(args) => handle_show(args, ops),
        ProjectSubcommand::Edit(args) => handle_edit(args, ops),
        ProjectSubcommand::Archive(args) => handle_archive(args, ops),
        ProjectSubcommand::Restore(args) => handle_restore(args, ops),
        ProjectSubcommand::Delete(args) => handle_delete(args, ops),
    }
}

// ── per-subcommand handlers ────────────────────────────────────────────────

fn handle_add(args: &ProjectAdd, ops: &ProjectOps) -> anyhow::Result<()> {
    let project = ops.create_project(NewProject {
        slug: args.slug.clone(),
        name: args.name.clone(),
        description: args.desc.clone(),
        status: ProjectStatus::Active,
    })?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&project)?),
        OutputFormat::Text => println!("Created project: {} ({})", project.name, project.slug),
    }
    Ok(())
}

fn handle_list(args: &ProjectList, ops: &ProjectOps) -> anyhow::Result<()> {
    let projects = ops.list_projects(args.status, args.archived)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&projects)?),
        OutputFormat::Text => {
            if projects.is_empty() {
                println!("No projects found.");
            } else {
                for p in &projects {
                    let archived = if p.archived_at.is_some() {
                        " [archived]"
                    } else {
                        ""
                    };
                    println!("{:<30} {} {}{}", p.slug, p.name, p.status, archived);
                }
            }
        }
    }
    Ok(())
}

fn handle_show(args: &ProjectShow, ops: &ProjectOps) -> anyhow::Result<()> {
    let project = ops
        .get_project(&args.slug)?
        .ok_or_else(|| anyhow::anyhow!("project '{}' not found", args.slug))?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&project)?),
        OutputFormat::Text => {
            println!("slug:        {}", project.slug);
            println!("name:        {}", project.name);
            println!("status:      {}", project.status);
            println!(
                "description: {}",
                project.description.as_deref().unwrap_or("—")
            );
            println!(
                "created:     {}",
                project.created_at.format("%Y-%m-%d %H:%M UTC")
            );
            println!(
                "updated:     {}",
                project.updated_at.format("%Y-%m-%d %H:%M UTC")
            );
            if let Some(at) = project.archived_at {
                println!("archived:    {}", at.format("%Y-%m-%d %H:%M UTC"));
            }
        }
    }
    Ok(())
}

fn handle_edit(args: &ProjectEdit, ops: &ProjectOps) -> anyhow::Result<()> {
    let patch = ProjectPatch {
        slug: args.new_slug.clone(),
        name: args.name.clone(),
        description: args.desc.clone(),
        clear_description: false,
        status: args.status,
    };
    let project = ops.update_project(&args.slug, patch)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&project)?),
        OutputFormat::Text => println!("Updated project: {} ({})", project.name, project.slug),
    }
    Ok(())
}

fn handle_archive(args: &ProjectArchive, ops: &ProjectOps) -> anyhow::Result<()> {
    let project = ops.archive_project(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&project)?),
        OutputFormat::Text => println!("Archived project: {}", project.slug),
    }
    Ok(())
}

fn handle_restore(args: &ProjectRestore, ops: &ProjectOps) -> anyhow::Result<()> {
    let project = ops.restore_project(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&project)?),
        OutputFormat::Text => println!("Restored project: {}", project.slug),
    }
    Ok(())
}

fn handle_delete(args: &ProjectDelete, ops: &ProjectOps) -> anyhow::Result<()> {
    ops.delete_project(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", json!({ "deleted": args.slug })),
        OutputFormat::Text => println!("Deleted project: {}", args.slug),
    }
    Ok(())
}
