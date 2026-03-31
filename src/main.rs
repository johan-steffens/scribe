// Rust guideline compliant 2026-02-21
//! Scribe application entry point.
//!
//! Sets the `mimalloc` global allocator, initialises structured tracing,
//! loads configuration, opens the database, and dispatches CLI subcommands.
//!
//! Running with no subcommand currently prints help; the TUI is Phase 3.

use std::process;
use std::sync::{Arc, Mutex};

use mimalloc::MiMalloc;
use tracing_subscriber::EnvFilter;

use clap::Parser;

mod cli;
mod config;
mod db;
mod domain;
mod ops;
mod store;

use cli::{Cli, Commands};
use ops::{ProjectOps, TaskOps};

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

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let config = config::Config::load()?;
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

    match cli.command {
        None => {
            // No subcommand — print help.
            // In Phase 3 this will launch the TUI instead.
            Cli::parse_from(["scribe", "--help"]);
        }
        Some(Commands::Project(cmd)) => {
            cli::project::run(&cmd, &project_ops)?;
        }
        Some(Commands::Task(cmd)) => {
            cli::task::run(&cmd, &task_ops, &project_ops)?;
        }
    }

    Ok(())
}
