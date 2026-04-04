// Common test helpers for integration and unit tests.
//
// These modules provide reusable utilities for creating temporary databases
// and mock configurations without requiring disk I/O or external services.
//
// # Example
//
// ```ignore
// use scribe::testing::db::open_in_memory;
// use scribe::testing::{todo_ops, task_store};
//
// let store = task_store::store();
// let ops = todo_ops::ops();
// ```
//
// Re-exports child modules for convenient access.
pub mod config;
pub mod db;
#[cfg(feature = "sync")]
pub mod keychain;

// ── ops layer re-exports ─────────────────────────────────────────────────

/// Re-exports from `crate::ops::todos::testing`.
#[doc(inline)]
pub use crate::ops::todos::testing as todo_ops;

/// Re-exports from `crate::ops::tasks::testing`.
#[doc(inline)]
pub use crate::ops::tasks::testing as task_ops;

/// Re-exports from `crate::ops::projects::testing`.
#[doc(inline)]
pub use crate::ops::projects::testing as project_ops;

/// Re-exports from `crate::ops::reminders::testing`.
#[doc(inline)]
pub use crate::ops::reminders::testing as reminder_ops;

/// Re-exports from `crate::ops::inbox::testing`.
#[doc(inline)]
pub use crate::ops::inbox::testing as inbox_ops;

/// Re-exports from `crate::ops::tracker::testing`.
#[doc(inline)]
pub use crate::ops::tracker::testing as tracker_ops;

// ── store layer re-exports ────────────────────────────────────────────────

/// Re-exports from `crate::store::todo_store::testing`.
#[doc(inline)]
pub use crate::store::todo_store::testing as todo_store;

/// Re-exports from `crate::store::task_store::testing`.
#[doc(inline)]
pub use crate::store::task_store::testing as task_store;

/// Re-exports from `crate::store::project_store::testing`.
#[doc(inline)]
pub use crate::store::project_store::testing as project_store;

/// Re-exports from `crate::store::reminder_store::testing`.
#[doc(inline)]
pub use crate::store::reminder_store::testing as reminder_store;

/// Re-exports from `crate::store::time_entry_store::testing`.
#[doc(inline)]
pub use crate::store::time_entry_store::testing as time_entry_store;

/// Re-exports from `crate::store::capture_store::testing`.
#[doc(inline)]
pub use crate::store::capture_store::testing as capture_store;

// ── domain re-exports ─────────────────────────────────────────────────────

/// Re-exports from `crate::domain::slug::testing`.
#[doc(inline)]
pub use crate::domain::slug::testing as slug;

// ── MCP re-exports ───────────────────────────────────────────────────────────

#[cfg(feature = "mcp")]
/// Re-exports from `crate::mcp::server::testing`.
#[doc(inline)]
pub use crate::mcp::server::testing as mcp;
