// Rust guideline compliant 2026-02-21
//! Scribe application entry point.
//!
//! Sets the `mimalloc` global allocator, initialises structured tracing,
//! loads configuration, opens the database, and dispatches to either the TUI
//! (no subcommand) or a CLI subcommand.

use std::process;

use mimalloc::MiMalloc;
use tracing_subscriber::EnvFilter;

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

    if let Err(err) = scribe::run() {
        eprintln!("error: {err:#}");
        // Exit code 1 = user/application error (M-APP-ERROR).
        process::exit(1);
    }
}
