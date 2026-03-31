// Rust guideline compliant 2026-02-21
//! CLI subcommands for time tracking (`scribe track …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::TrackerOps`].
//! All subcommands support `--output json` for machine-readable output.

use clap::{Args, Subcommand};

use crate::cli::project::OutputFormat;
use crate::ops::{ProjectOps, TaskOps, TrackerOps};

// ── top-level track command ────────────────────────────────────────────────

/// Arguments for the `scribe track` subcommand group.
#[derive(Debug, Args)]
pub struct TrackCommand {
    /// Track subcommand.
    #[command(subcommand)]
    pub subcommand: TrackSubcommand,
}

/// All `scribe track` subcommands.
#[derive(Debug, Subcommand)]
pub enum TrackSubcommand {
    /// Start a new timer.
    Start(TrackStart),
    /// Stop the running timer.
    Stop(TrackStop),
    /// Show the current timer status.
    Status(TrackStatus),
    /// Show a time report.
    Report(TrackReport),
}

// ── subcommand structs ─────────────────────────────────────────────────────

/// Arguments for `scribe track start`.
#[derive(Debug, Args)]
pub struct TrackStart {
    /// Project slug (defaults to `quick-capture`).
    #[arg(long)]
    pub project: Option<String>,
    /// Optional task slug to link the timer to.
    #[arg(long)]
    pub task: Option<String>,
    /// Optional free-text note.
    #[arg(long)]
    pub note: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe track stop`.
#[derive(Debug, Args)]
pub struct TrackStop {
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe track status`.
#[derive(Debug, Args)]
pub struct TrackStatus {
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe track report`.
#[derive(Debug, Args)]
pub struct TrackReport {
    /// Restrict to today's entries.
    #[arg(long)]
    pub today: bool,
    /// Restrict to this week's entries.
    #[arg(long)]
    pub week: bool,
    /// Filter by project slug.
    #[arg(long)]
    pub project: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes a `track` subcommand against the given ops layers.
///
/// `project_ops` is used to resolve project slugs, `task_ops` to resolve task
/// slugs. Prints results to stdout; errors are propagated to the caller.
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. timer already running, DB error).
pub fn run(
    cmd: &TrackCommand,
    ops: &TrackerOps,
    project_ops: &ProjectOps,
    task_ops: &TaskOps,
) -> anyhow::Result<()> {
    match &cmd.subcommand {
        TrackSubcommand::Start(args) => handlers::handle_start(args, ops, task_ops),
        TrackSubcommand::Stop(args) => handlers::handle_stop(args, ops),
        TrackSubcommand::Status(args) => handlers::handle_status(args, ops),
        TrackSubcommand::Report(args) => handlers::handle_report(args, ops, project_ops),
    }
}

// ── per-subcommand handlers ────────────────────────────────────────────────

#[path = "track_handlers.rs"]
mod handlers;
