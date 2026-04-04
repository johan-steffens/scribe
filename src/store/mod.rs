//! `SQLite` repository implementations for all domain entities.
//!
//! Each sub-module provides a concrete `Sqlite*` struct that implements the
//! corresponding repository trait from [`crate::domain`]. All structs hold an
//! `Arc<Mutex<rusqlite::Connection>>` and are `Clone`-able as cheap handles
//! to the same underlying connection.
//!
//! Raw `rusqlite` errors are never exposed through public APIs; they are
//! mapped to `anyhow::Error` at the store boundary (M-DONT-LEAK-TYPES).

pub mod capture_store;
pub mod project_store;
pub mod reminder_store;
pub mod task_store;
pub mod time_entry_store;
pub mod todo_store;

// Phase 1 stores — wired into the CLI.
#[doc(inline)]
pub use project_store::SqliteProjects;
#[doc(inline)]
pub use task_store::SqliteTasks;

// Phase 2 stores — wired into the CLI.
#[doc(inline)]
pub use capture_store::SqliteCaptureItems;
#[doc(inline)]
pub use reminder_store::SqliteReminders;
#[doc(inline)]
pub use time_entry_store::SqliteTimeEntries;
#[doc(inline)]
pub use todo_store::SqliteTodos;
