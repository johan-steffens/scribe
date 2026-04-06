//! CLI subcommands for the quick-capture inbox (`scribe inbox …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::InboxOps`].  The
//! `process` subcommand is interactive (reads from stdin) unless `--output json`
//! is passed.  When stdin is a TTY, tab completion for project slugs is provided
//! via [`crate::cli::prompt`].  The `report` subcommand is an alias for
//! `scribe report inbox`.

use std::sync::{Arc, Mutex};

use clap::{Args, Subcommand};
use rusqlite::Connection;

use crate::cli::project::OutputFormat;
use crate::cli::prompt;
use crate::cli::report::{ReportCommand, ReportSubcommand, ReportSubcommandCommon};
use crate::cli::report_handlers::handle_report as report_handle_report;
use crate::ops::InboxOps;
use crate::ops::inbox::ProcessAction;

use std::io::{BufRead, Write};

// ── top-level inbox command ────────────────────────────────────────────────

/// Arguments for the `scribe inbox` subcommand group.
#[derive(Debug, Args)]
pub struct InboxCommand {
    /// Inbox subcommand.
    #[command(subcommand)]
    pub subcommand: InboxSubcommand,
}

/// All `scribe inbox` subcommands.
#[derive(Debug, Subcommand)]
pub enum InboxSubcommand {
    /// List unprocessed inbox items.
    List(InboxList),
    /// Process an inbox item interactively.
    Process(InboxProcess),
    /// Generate a report for the inbox (alias for `scribe report inbox`).
    Report(InboxReport),
}

// ── subcommand structs ─────────────────────────────────────────────────────

/// Arguments for `scribe inbox list`.
#[derive(Debug, Args)]
pub struct InboxList {
    /// Include already-processed items.
    #[arg(long)]
    pub all: bool,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe inbox process`.
#[derive(Debug, Args)]
pub struct InboxProcess {
    /// Capture item slug to process.
    pub slug: String,
    /// Output format.
    ///
    /// When `json`, the raw item is returned without entering interactive mode.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe inbox report`.
#[derive(Debug, Args)]
pub struct InboxReport {
    /// Restrict the report to items created today.
    #[arg(long)]
    pub today: bool,
    /// Restrict the report to items created this week.
    #[arg(long)]
    pub week: bool,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
    /// Include detailed information in the report.
    #[arg(long)]
    pub detailed: bool,
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes an `inbox` subcommand against the given ops layer.
///
/// `conn` is forwarded to interactive prompts so that tab completion can
/// query the database for project slugs. It is also required for the `report`
/// subcommand which delegates to [`crate::cli::report_handlers::handle_report`].
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. item not found, DB error).
pub fn run(
    cmd: &InboxCommand,
    ops: &InboxOps,
    conn: &Arc<Mutex<Connection>>,
) -> anyhow::Result<()> {
    match &cmd.subcommand {
        InboxSubcommand::List(args) => handle_list(args, ops),
        InboxSubcommand::Process(args) => handle_process(args, ops, conn),
        InboxSubcommand::Report(args) => handle_report(args, conn),
    }
}

// ── handlers ───────────────────────────────────────────────────────────────

fn handle_list(args: &InboxList, ops: &InboxOps) -> anyhow::Result<()> {
    let items = ops.list(args.all)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&items)?),
        OutputFormat::Text => {
            if items.is_empty() {
                println!("Inbox is empty.");
            } else {
                for item in &items {
                    let processed = if item.processed { " [processed]" } else { "" };
                    println!("{:<35} {}{}", item.slug, item.body, processed);
                }
            }
        }
    }
    Ok(())
}

fn handle_process(
    args: &InboxProcess,
    ops: &InboxOps,
    conn: &Arc<Mutex<Connection>>,
) -> anyhow::Result<()> {
    let item = ops
        .get(&args.slug)?
        .ok_or_else(|| anyhow::anyhow!("capture item '{}' not found", args.slug))?;

    if args.output == OutputFormat::Json {
        // Non-interactive: just return the raw item.
        println!("{}", serde_json::to_string_pretty(&item)?);
        return Ok(());
    }

    // Interactive mode — prompt user for action.
    println!("Item: {}", item.body);
    println!("  [1] Convert to task");
    println!("  [2] Convert to todo");
    println!("  [3] Assign to project");
    println!("  [4] Discard");
    print!("Choice: ");

    std::io::stdout()
        .flush()
        .map_err(|e| anyhow::anyhow!("flush failed: {e}"))?;

    let stdin = std::io::stdin();
    let choice_line = stdin
        .lock()
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no input provided"))?
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let choice = choice_line.trim().to_owned();

    let action = match choice.as_str() {
        "1" => {
            let project_slug = prompt::prompt_project_slug("Project slug: ", conn)?;
            let title_input = prompt::prompt("Title (leave blank to use body): ")?;
            let title = if title_input.is_empty() {
                None
            } else {
                Some(title_input)
            };
            ProcessAction::ConvertToTask {
                project_slug,
                title,
                priority: None,
            }
        }
        "2" => {
            let project_slug = prompt::prompt_project_slug("Project slug: ", conn)?;
            let title_input = prompt::prompt("Title (leave blank to use body): ")?;
            let title = if title_input.is_empty() {
                None
            } else {
                Some(title_input)
            };
            ProcessAction::ConvertToTodo {
                project_slug,
                title,
            }
        }
        "3" => {
            let project_slug = prompt::prompt_project_slug("Project slug: ", conn)?;
            ProcessAction::AssignToProject { project_slug }
        }
        "4" => ProcessAction::Discard,
        other => {
            return Err(anyhow::anyhow!("invalid choice '{other}'"));
        }
    };

    let processed = ops.process(&args.slug, action)?;
    println!("Processed: {}", processed.slug);
    Ok(())
}

/// Delegates to [`report_handle_report`] after building a [`ReportCommand`]
/// from the `InboxReport` arguments.
fn handle_report(args: &InboxReport, conn: &Arc<Mutex<Connection>>) -> anyhow::Result<()> {
    let cmd = ReportCommand {
        subcommand: Some(ReportSubcommand::Inbox {
            common: ReportSubcommandCommon {
                output: args.output.clone(),
                detailed: args.detailed,
            },
        }),
        today: args.today,
        week: args.week,
        output: args.output.clone(),
        detailed: args.detailed,
    };
    report_handle_report(&cmd, Arc::clone(conn))
}
