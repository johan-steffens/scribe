// Rust guideline compliant 2026-02-21
//! CLI argument definitions and top-level command dispatch.
//!
//! This module defines the [`Cli`] struct (the root `clap` parser) and the
//! [`Commands`] enum listing all top-level subcommands. Subcommand argument
//! structs live in their own sub-modules.
//!
//! # Usage
//!
//! ```no_run
//! use clap::Parser;
//! use scribe::cli::Cli;
//!
//! let cli = Cli::parse();
//! ```

pub mod project;
pub mod task;

use clap::{Parser, Subcommand};

#[doc(inline)]
pub use project::ProjectCommand;
#[doc(inline)]
pub use task::TaskCommand;

/// Scribe — personal productivity CLI/TUI tool.
///
/// Run without arguments to open the TUI (Phase 3).
#[derive(Debug, Parser)]
#[command(
    name = "scribe",
    about = "Personal productivity CLI/TUI tool",
    version,
    propagate_version = true
)]
pub struct Cli {
    /// Top-level subcommand.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// All top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Manage projects.
    Project(ProjectCommand),
    /// Manage tasks.
    Task(TaskCommand),
}
