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

pub mod agent;
pub mod capture;
pub mod complete;
pub mod daemon;
pub mod inbox;
pub mod parse;
pub mod project;
pub mod reminder;
pub mod task;
pub mod todo;
pub mod track;

use clap::{Parser, Subcommand};

#[doc(inline)]
pub use agent::AgentCommand;
#[doc(inline)]
pub use capture::CaptureCommand;
#[doc(inline)]
pub use complete::CompletionShell;
#[doc(inline)]
pub use inbox::InboxCommand;
#[doc(inline)]
pub use project::ProjectCommand;
#[doc(inline)]
pub use reminder::ReminderCommand;
#[doc(inline)]
pub use task::TaskCommand;
#[doc(inline)]
pub use todo::TodoCommand;
#[doc(inline)]
pub use track::TrackCommand;

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
    /// Manage todos.
    Todo(TodoCommand),
    /// Time tracking.
    Track(TrackCommand),
    /// Quickly capture a thought into the inbox.
    Capture(CaptureCommand),
    /// Manage the quick-capture inbox.
    Inbox(InboxCommand),
    /// Manage reminders.
    Reminder(ReminderCommand),
    /// Run the background reminder notification daemon.
    ///
    /// Polls for due reminders and fires OS desktop notifications.
    /// Intended to be registered with launchd (macOS) or systemd (Linux).
    Daemon {
        /// Polling interval in seconds (default: 30).
        #[arg(long, short = 'i')]
        interval: Option<u64>,
    },
    /// Install skill files for AI coding agents.
    Agent {
        /// Agent subcommand.
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Print a shell completion script for the given shell.
    Completions {
        /// Shell to generate completions for.
        shell: CompletionShell,
    },
    /// Run the Scribe MCP stdio server (requires the `mcp` feature).
    ///
    /// Connect your AI agent to this process.  Stdout is the MCP wire
    /// protocol — do NOT run this in a plain terminal.
    #[cfg(feature = "mcp")]
    Mcp,
}
