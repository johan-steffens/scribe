// Rust guideline compliant 2026-02-21
//! Scribe application entry point.
//!
//! Sets the `mimalloc` global allocator, initialises structured tracing,
//! loads configuration, opens the database, and dispatches to either the TUI
//! (no subcommand) or a CLI subcommand.

use std::process;
use std::sync::{Arc, Mutex};

use mimalloc::MiMalloc;
use tracing_subscriber::EnvFilter;

use clap::Parser;

mod cli;
mod config;
mod db;
mod domain;
#[cfg(feature = "mcp")]
mod mcp;
mod notify;
mod ops;
#[cfg(feature = "sync")]
mod server;
mod store;
#[cfg(feature = "sync")]
mod sync;
mod tui;

use cli::{Cli, Commands};
use ops::{InboxOps, ProjectOps, ReminderOps, TaskOps, TodoOps, TrackerOps};

/// Global allocator — provides significant performance gains (M-MIMALLOC-APPS).
// DOCUMENTED-MAGIC: MiMalloc replaces the system allocator for up to ~25%
// throughput improvement on allocation-heavy paths; no code changes required
// beyond this declaration.
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    // Initialise tracing with an env-filter so users can set RUST_LOG.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        // Exit code 1 = user/application error (M-APP-ERROR).
        process::exit(1);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "top-level command dispatch; extracting further would reduce readability"
)]
fn run() -> anyhow::Result<()> {
    // Intercept `scribe __complete <entity>` before clap parsing.
    //
    // The double-underscore prefix in `__complete` confuses the bash
    // completion generator inside `clap_complete` (it uses `__` as a path
    // separator internally), causing a panic when generating bash completions.
    // By handling `__complete` here — from raw OS args — we keep it out of
    // the clap `Cli` command tree entirely, so all five shells work correctly.
    let mut args = std::env::args_os();
    let _bin = args.next(); // skip argv[0]
    if let Some(first) = args.next()
        && first == "__complete"
    {
        let entity_os = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("__complete requires an entity argument"))?;
        let entity_str = entity_os.to_string_lossy();
        let entity =
            clap::ValueEnum::from_str(&entity_str, true /* case-insensitive */).map_err(
                |_ignored| {
                    anyhow::anyhow!(
                        "unknown entity '{entity_str}'; valid values: projects, tasks, todos, reminders, captures, entries",
                    )
                },
            )?;
        return cli::complete::run_complete(entity);
    }

    let cli = Cli::parse();

    // Handle `completions` before opening the DB — no database access needed.
    if let Some(Commands::Completions { shell }) = &cli.command {
        cli::complete::run_completions(*shell);
        return Ok(());
    }

    // Load config — needed by setup, service, and agent commands.
    let mut config = config::Config::load()?;

    // `scribe setup` — wizard / status, no DB access needed.
    if let Some(Commands::Setup(ref args)) = cli.command {
        return cli::setup::run(args, &mut config);
    }

    // `scribe service <cmd>` — no DB access needed.
    if let Some(Commands::Service { command: ref cmd }) = cli.command {
        return cli::service::run(cmd, &mut config);
    }

    // `scribe agent install` — no DB access needed.
    if let Some(Commands::Agent {
        command: cli::AgentCommand::Install(ref args),
    }) = cli.command
    {
        return cli::agent::run(args, &mut config);
    }

    // `scribe sync configure` and `scribe sync status` — no DB access needed.
    // The one-shot `scribe sync` (no subcommand) is handled after DB open below.
    #[cfg(feature = "sync")]
    if let Some(Commands::Sync(ref cmd)) = cli.command
        && cmd.subcommand.is_some()
    {
        return cli::sync::run(cmd, &mut config, None);
    }

    // Allow integration tests to inject an isolated DB path without modifying
    // the user's real database. SCRIBE_TEST_DB is read only when present.
    let db_path = if let Ok(p) = std::env::var("SCRIBE_TEST_DB") {
        std::path::PathBuf::from(p)
    } else {
        config.db_path()
    };

    let conn = db::open(&db_path)?;
    let conn = Arc::new(Mutex::new(conn));

    let project_ops = ProjectOps::new(&conn);
    let task_ops = TaskOps::new(Arc::clone(&conn));
    let todo_ops = TodoOps::new(Arc::clone(&conn));
    let tracker_ops = TrackerOps::new(Arc::clone(&conn));
    let inbox_ops = InboxOps::new(&conn);
    let reminder_ops = ReminderOps::new(Arc::clone(&conn));

    // Fire any due reminders on startup and send OS notifications.
    if let Ok(due) = reminder_ops.check_due() {
        for r in &due {
            tracing::info!(reminder.slug = %r.slug, "reminder fired");
            notify::fire(r);
        }
    }

    match cli.command {
        None => {
            // No subcommand — launch the TUI.
            tui::run(Arc::clone(&conn), &config)?;
        }
        Some(Commands::Project(cmd)) => {
            cli::project::run(&cmd, &project_ops)?;
        }
        Some(Commands::Task(cmd)) => {
            cli::task::run(&cmd, &task_ops, &project_ops)?;
        }
        Some(Commands::Todo(cmd)) => {
            cli::todo::run(&cmd, &todo_ops, &project_ops)?;
        }
        Some(Commands::Track(cmd)) => {
            cli::track::run(&cmd, &tracker_ops, &project_ops, &task_ops)?;
        }
        Some(Commands::Capture(cmd)) => {
            cli::capture::run(&cmd, &inbox_ops)?;
        }
        Some(Commands::Inbox(cmd)) => {
            cli::inbox::run(&cmd, &inbox_ops, &conn)?;
        }
        Some(Commands::Reminder(cmd)) => {
            cli::reminder::run(&cmd, &reminder_ops, &project_ops)?;
        }
        Some(Commands::Daemon { interval }) => {
            cli::daemon::run(&conn, interval, &config)?;
        }
        // Handled above before the DB opens.
        Some(
            Commands::Setup(_)
            | Commands::Service { .. }
            | Commands::Agent { .. }
            | Commands::Completions { .. },
        ) => {}
        #[cfg(feature = "sync")]
        Some(Commands::Sync(ref cmd)) => {
            cli::sync::run(cmd, &mut config, Some(&conn))?;
        }
        #[cfg(feature = "mcp")]
        Some(Commands::Mcp) => {
            mcp::run(&conn, &config)?;
        }
    }

    Ok(())
}
