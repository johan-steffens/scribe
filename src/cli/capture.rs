// Rust guideline compliant 2026-02-21
//! CLI subcommand for quick capture (`scribe capture <text>`).
//!
//! A single one-liner command that stores a raw thought in the inbox without
//! requiring any project context. Items are later triaged with `scribe inbox`.

use clap::Args;

use crate::cli::project::OutputFormat;
use crate::ops::InboxOps;

// ── top-level capture command ──────────────────────────────────────────────

/// Arguments for `scribe capture`.
#[derive(Debug, Args)]
pub struct CaptureCommand {
    /// Text to capture into the inbox.
    pub text: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes the `capture` command against the given ops layer.
///
/// Prints the created capture item on success.
///
/// # Errors
///
/// Returns an error if the body is empty after trimming or a database error
/// occurs.
pub fn run(cmd: &CaptureCommand, ops: &InboxOps) -> anyhow::Result<()> {
    let item = ops.capture(&cmd.text)?;
    match cmd.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&item)?),
        OutputFormat::Text => println!("Captured: {} [{}]", item.body, item.slug),
    }
    Ok(())
}
