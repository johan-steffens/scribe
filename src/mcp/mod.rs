//! MCP (Model Context Protocol) stdio server for Scribe.
//!
//! This module is only compiled when the `mcp` Cargo feature is enabled.
//! It exposes all Scribe data as MCP tools and resources that AI agents can
//! call directly — without spawning a subprocess for every operation.
//!
//! # Architecture
//!
//! `rmcp` requires an async runtime (`tokio`).  The rest of the Scribe
//! codebase is fully synchronous.  The [`run`] function bridges the two
//! worlds:
//!
//! 1. It opens the Scribe DB synchronously in `main.rs` before calling `run`.
//! 2. `run` wraps the connection in `Arc<Mutex<Connection>>` and creates all
//!    ops structs.
//! 3. It constructs a `tokio` runtime and runs the `rmcp` stdio server on it.
//! 4. The server blocks until the MCP client disconnects.
//!
//! # Transport
//!
//! Uses the `transport-io` feature of `rmcp` which reads JSON-RPC messages
//! from stdin and writes them to stdout.  **Do not run `scribe mcp` in a
//! plain terminal** — stdout is the MCP wire protocol.
//!
//! # Feature gate
//!
//! All items in this module are compiled only with `--features mcp`.

#[cfg(feature = "mcp")]
pub mod server;

#[cfg(feature = "mcp")]
pub use server::{
    CaptureParams, InboxProcessParams, ProjectCreateParams, ProjectSlugParam, ReminderCreateParams,
    ReminderListParams, ReminderSlugParam, TaskCreateParams, TaskListParams, TaskSlugParam,
    TimerStartParams, TodoCreateParams, TodoListParams, TodoSlugParam, TrackReportParams,
};

#[cfg(feature = "mcp")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "mcp")]
use rusqlite::Connection;

#[cfg(feature = "mcp")]
use crate::config::Config;

/// Starts the Scribe MCP stdio server and blocks until the client disconnects.
///
/// A startup message is printed to stderr (stdout is reserved for the MCP wire
/// protocol).  The function creates a `tokio` runtime internally so the caller
/// does not need to be async.
///
/// # Errors
///
/// Returns an error if the `tokio` runtime cannot be created or if the MCP
/// server fails during initialisation.
#[cfg(feature = "mcp")]
pub fn run(conn: &Arc<Mutex<Connection>>, _config: &Config) -> anyhow::Result<()> {
    eprintln!("Scribe MCP server running on stdio. Connect your agent to this process.");

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("failed to create tokio runtime: {e}"))?;

    rt.block_on(async { server::serve(conn).await })
}
