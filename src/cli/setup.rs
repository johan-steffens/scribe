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
//! 3. **State sync** (optional, requires `sync` feature) — configure a
//!    remote sync provider so data is backed up and shared across machines.
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
use crate::cli::service::{ServiceCommand, ServiceState};
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
    println!();

    crate::cli::service::status(config);
    println!();

    let agents = if config.setup.agent_installed {
        "installed"
    } else {
        "not installed  (run `scribe agent install`)"
    };

    println!("  Agent integration: {agents}");
    #[cfg(feature = "sync")]
    print_sync_status(config);
    println!();

    if !config.setup.daemon_service_installed || !config.setup.agent_installed {
        println!("Run `scribe setup` to configure missing items.");
    } else {
        println!("All setup steps are complete.");
    }
}

// ── sync status helper (sync feature only) ────────────────────────────────

/// Prints the sync configuration status line.
#[cfg(feature = "sync")]
fn print_sync_status(config: &Config) {
    let sync_status = if config.sync.enabled {
        format!("configured (provider: {})", config.sync.provider)
    } else {
        "not configured  (run `scribe sync configure`)".to_owned()
    };
    println!("  Sync:              {sync_status}");
}

// ── repair detection ───────────────────────────────────────────────────────

fn check_and_offer_repair(config: &mut Config) -> anyhow::Result<Option<bool>> {
    let state = ServiceState::check(config);

    if !state.has_discrepancy() {
        return Ok(None);
    }

    println!("⚠ WARNING: Broken service state detected!");
    println!();

    if state.config_says_installed && !state.file_exists {
        println!("The daemon service is marked as installed in your config,");
        println!("but the service definition file is missing from disk.");
        println!();
        println!("This can happen after a binary upgrade or manual file deletion.");
    } else if !state.config_says_installed && state.file_exists {
        println!("A daemon service file exists on disk,");
        println!("but your config does not have it marked as installed.");
        println!();
        println!("This can happen after an incomplete uninstall.");
    }

    println!();
    println!("Would you like to repair this now?");

    let repair = prompt_yes_no_default("  [R]epair / [C]ancel", 'r')?;

    if repair {
        println!();
        let service_cmd = if state.config_says_installed {
            ServiceCommand::Reinstall
        } else {
            ServiceCommand::Uninstall
        };
        crate::cli::service::run(&service_cmd, config, None)?;
    }

    Ok(Some(repair))
}

fn prompt_yes_no_default(prompt: &str, default: char) -> anyhow::Result<bool> {
    use std::io::Write;

    let hint = "[R/c]";
    print!("{prompt} {hint} ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    let trimmed = line.trim().to_lowercase();
    Ok(match trimmed.as_str() {
        "r" | "repair" => true,
        "c" | "cancel" => false,
        _ => default == 'r',
    })
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

    let repair_done = check_and_offer_repair(config)?;
    if repair_done == Some(false) {
        println!("Setup cancelled.");
        return Ok(());
    }

    let mut anything_done = repair_done == Some(true);

    // ── Step 1: daemon service ─────────────────────────────────────────────
    if config.setup.daemon_service_installed {
        #[cfg(not(feature = "sync"))]
        println!("[1/2] Daemon service: already installed — skipping.");
        #[cfg(feature = "sync")]
        println!("[1/3] Daemon service: already installed — skipping.");
    } else {
        #[cfg(not(feature = "sync"))]
        println!("[1/2] Daemon service");
        #[cfg(feature = "sync")]
        println!("[1/3] Daemon service");
        println!("      Installs `scribe daemon` as a background service so that");
        println!("      reminder notifications are delivered automatically.");
        println!("      On macOS: launchd user agent (no sudo required).");
        println!("      On Linux: systemd user unit (no sudo required).");
        println!();

        if prompt_yes_no("      Install the daemon service?", true)? {
            println!();
            crate::cli::service::run(&ServiceCommand::Install, config, None)?;
            anything_done = true;
        } else {
            println!("      Skipped. You can install later with `scribe service install`.");
        }
    }

    println!();

    // ── Step 2: agent integration ──────────────────────────────────────────
    if config.setup.agent_installed {
        #[cfg(not(feature = "sync"))]
        println!("[2/2] Agent integration: already installed — skipping.");
        #[cfg(feature = "sync")]
        println!("[2/3] Agent integration: already installed — skipping.");
    } else {
        #[cfg(not(feature = "sync"))]
        println!("[2/2] Agent integration");
        #[cfg(feature = "sync")]
        println!("[2/3] Agent integration");
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

    // ── Step 3: sync configuration (sync feature only) ────────────────────
    #[cfg(feature = "sync")]
    {
        println!();
        if config.sync.enabled {
            println!(
                "[3/3] State sync: already configured (provider: {}) — skipping.",
                config.sync.provider
            );
        } else {
            println!("[3/3] State sync");
            println!("      Configures a remote sync provider to back up your data");
            println!("      and share it across multiple machines.");
            println!();

            if prompt_yes_no("      Configure state sync?", false)? {
                println!();
                let cfg_args = crate::cli::sync::SyncConfigureArgs {
                    provider: None,
                    remove: false,
                    gist_id: None,
                    output: OutputFormat::Text,
                };
                crate::cli::sync::run_configure(&cfg_args, config)?;
                anything_done = true;
            } else {
                println!("      Skipped. You can configure later with `scribe sync configure`.");
            }
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
