// Rust guideline compliant 2026-02-21
//! `scribe setup` — first-run wizard and setup status.
//!
//! On first run this command presents an interactive wizard that guides the
//! user through optional setup steps:
//!
//! 1. **Daemon service** — install `scribe daemon` as a background service
//!    so that reminder notifications are delivered automatically.
//! 2. **Agent integration** — install the Scribe skill file to detected AI
//!    coding agent directories.
//!
//! If setup has already been completed, the command prints a status summary
//! instead. The user can force the wizard again with `--wizard`.
//!
//! # Usage
//!
//! ```sh
//! scribe setup              # wizard on first run, status thereafter
//! scribe setup --wizard     # always run the wizard
//! scribe setup --status     # always show status (skip wizard)
//! ```

use clap::Args;

use crate::cli::agent::AgentInstallArgs;
use crate::cli::project::OutputFormat;
use crate::cli::service::ServiceCommand;
use crate::config::Config;

// ── clap args ──────────────────────────────────────────────────────────────

/// Arguments for `scribe setup`.
#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Always run the interactive wizard, even if setup was already done.
    #[arg(long)]
    pub wizard: bool,
    /// Print setup status and exit without running the wizard.
    #[arg(long)]
    pub status: bool,
}

// ── entry point ────────────────────────────────────────────────────────────

/// Executes `scribe setup`.
///
/// Runs the interactive wizard on first run (or when `--wizard` is passed),
/// or prints the current setup status.
///
/// # Errors
///
/// Returns an error if a sub-step (service install, agent install) fails, or
/// if the config cannot be saved.
pub fn run(args: &SetupArgs, config: &mut Config) -> anyhow::Result<()> {
    let already_done = config.setup.daemon_service_installed || config.setup.agent_installed;

    if args.status || (already_done && !args.wizard) {
        print_status(config);
        return Ok(());
    }

    run_wizard(config)
}

// ── status display ─────────────────────────────────────────────────────────

/// Prints a compact summary of what has and has not been set up.
pub fn print_status(config: &Config) {
    println!("Scribe setup status");
    println!("-------------------");

    let daemon = if config.setup.daemon_service_installed {
        "installed"
    } else {
        "not installed  (run `scribe service install`)"
    };

    let agents = if config.setup.agent_installed {
        "installed"
    } else {
        "not installed  (run `scribe agent install`)"
    };

    println!("  Daemon service:    {daemon}");
    println!("  Agent integration: {agents}");
    println!();

    if !config.setup.daemon_service_installed || !config.setup.agent_installed {
        println!("Run `scribe setup` to configure missing items.");
    } else {
        println!("All setup steps are complete.");
    }
}

// ── wizard ─────────────────────────────────────────────────────────────────

/// Runs the interactive setup wizard.
fn run_wizard(config: &mut Config) -> anyhow::Result<()> {
    println!("Welcome to Scribe setup!");
    println!("========================");
    println!();
    println!("This wizard will help you configure optional features.");
    println!("Press Enter to accept the default shown in [brackets].");
    println!();

    let mut anything_done = false;

    // ── Step 1: daemon service ─────────────────────────────────────────────
    if config.setup.daemon_service_installed {
        println!("[1/2] Daemon service: already installed — skipping.");
    } else {
        println!("[1/2] Daemon service");
        println!("      Installs `scribe daemon` as a background service so that");
        println!("      reminder notifications are delivered automatically.");
        println!("      On macOS: launchd user agent (no sudo required).");
        println!("      On Linux: systemd user unit (no sudo required).");
        println!();

        if prompt_yes_no("      Install the daemon service?", true)? {
            println!();
            crate::cli::service::run(&ServiceCommand::Install, config)?;
            anything_done = true;
        } else {
            println!("      Skipped. You can install later with `scribe service install`.");
        }
    }

    println!();

    // ── Step 2: agent integration ──────────────────────────────────────────
    if config.setup.agent_installed {
        println!("[2/2] Agent integration: already installed — skipping.");
    } else {
        println!("[2/2] Agent integration");
        println!("      Installs a Scribe skill file to detected AI coding agent");
        println!("      directories (Claude Code, Cursor, Codex, Windsurf).");
        println!();

        if prompt_yes_no("      Install agent integration?", true)? {
            println!();
            let agent_args = AgentInstallArgs {
                output: OutputFormat::Text,
            };
            crate::cli::agent::run(&agent_args, config)?;
            anything_done = true;
        } else {
            println!("      Skipped. You can install later with `scribe agent install`.");
        }
    }

    println!();

    if anything_done {
        println!("Setup complete. Run `scribe setup --status` to review.");
    } else {
        println!("Nothing was installed. Run `scribe setup --status` to review.");
    }

    Ok(())
}

// ── prompt helper ──────────────────────────────────────────────────────────

/// Reads a yes/no answer from stdin.
///
/// Displays `prompt` followed by `[Y/n]` or `[y/N]` depending on `default`.
/// An empty input (just Enter) accepts the default. Returns `true` for yes,
/// `false` for no.
///
/// # Errors
///
/// Returns an error if stdin cannot be read.
fn prompt_yes_no(prompt: &str, default: bool) -> anyhow::Result<bool> {
    use std::io::Write;

    let hint = if default { "[Y/n]" } else { "[y/N]" };
    print!("{prompt} {hint} ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    let trimmed = line.trim().to_lowercase();
    Ok(match trimmed.as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        "" => default,
        _ => {
            // Unrecognised input — treat as default.
            default
        }
    })
}
