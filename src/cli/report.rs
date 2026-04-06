//! CLI subcommand definitions for the reporting system (`scribe report …`).
//!
//! Provides centralized reporting across all Scribe domains with support for
//! summary reports, domain-specific reports, and flexible output formats.

use clap::{Args, Subcommand};

use crate::cli::project::OutputFormat;

/// Arguments for the `scribe report` command.
#[derive(Debug, Args)]
pub struct ReportCommand {
    /// Report subcommand specifying which domain to report on.
    #[command(subcommand)]
    pub subcommand: Option<ReportSubcommand>,

    /// Restrict the report to items due or created today.
    #[arg(long)]
    pub today: bool,

    /// Restrict the report to items due or created this week.
    #[arg(long)]
    pub week: bool,

    /// Output format for the report.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,

    /// Include detailed information in the report.
    #[arg(long)]
    pub detailed: bool,
}

/// Common flags for report subcommands.
#[derive(Debug, Args)]
pub struct ReportSubcommandCommon {
    /// Output format for the report.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,

    /// Include detailed information in the report.
    #[arg(long)]
    pub detailed: bool,
}

/// All `scribe report` subcommands.
#[derive(Debug, Subcommand)]
pub enum ReportSubcommand {
    /// Generate a report for a specific project.
    Project {
        /// Project slug to report on.
        slug: String,
        #[command(flatten)]
        common: ReportSubcommandCommon,
    },
    /// Generate a report for a specific task.
    Task {
        /// Task slug to report on.
        slug: String,
        #[command(flatten)]
        common: ReportSubcommandCommon,
    },
    /// Generate a report for a specific todo.
    Todo {
        /// Todo slug to report on.
        slug: String,
        #[command(flatten)]
        common: ReportSubcommandCommon,
    },
    /// Generate a report for the inbox.
    Inbox {
        #[command(flatten)]
        common: ReportSubcommandCommon,
    },
    /// Generate a report for reminders.
    Reminders {
        #[command(flatten)]
        common: ReportSubcommandCommon,
    },
    /// Generate a time tracking report (alias for `scribe track report`).
    Track {
        /// Restrict to today's entries.
        #[arg(long)]
        today: bool,
        /// Restrict to this week's entries.
        #[arg(long)]
        week: bool,
        /// Filter by project slug.
        #[arg(long)]
        project: Option<String>,
        /// Output format.
        #[arg(long, default_value = "text")]
        output: OutputFormat,
    },
}
