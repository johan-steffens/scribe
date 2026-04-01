// Rust guideline compliant 2026-02-21
//! CLI subcommands for the quick-capture inbox (`scribe inbox …`).
//!
//! Each subcommand maps to an operation in [`crate::ops::InboxOps`].
//! The `process` subcommand is interactive (reads from stdin) unless
//! `--output json` is passed.

use clap::{Args, Subcommand};

use crate::cli::project::OutputFormat;
use crate::ops::inbox::ProcessAction;
use crate::ops::InboxOps;

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

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes an `inbox` subcommand against the given ops layer.
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. item not found, DB error).
pub fn run(cmd: &InboxCommand, ops: &InboxOps) -> anyhow::Result<()> {
    match &cmd.subcommand {
        InboxSubcommand::List(args) => handle_list(args, ops),
        InboxSubcommand::Process(args) => handle_process(args, ops),
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

fn handle_process(args: &InboxProcess, ops: &InboxOps) -> anyhow::Result<()> {
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
            let project_slug = prompt_line("Project slug: ")?;
            let title_input = prompt_line("Title (leave blank to use body): ")?;
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
            let project_slug = prompt_line("Project slug: ")?;
            let title_input = prompt_line("Title (leave blank to use body): ")?;
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
            let project_slug = prompt_line("Project slug: ")?;
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

/// Prints `prompt` to stdout, flushes, reads a trimmed line from stdin.
fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    print!("{prompt}");
    std::io::stdout()
        .flush()
        .map_err(|e| anyhow::anyhow!("flush: {e}"))?;
    let stdin = std::io::stdin();
    let line = stdin
        .lock()
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no input"))?
        .map_err(|e| anyhow::anyhow!("read: {e}"))?;
    Ok(line.trim().to_owned())
}
