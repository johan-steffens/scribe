// Rust guideline compliant 2026-02-21
//! Business logic operations for all entities.
//!
//! The `ops` layer sits between the CLI and the store layer. It enforces
//! business rules that span multiple repository calls — e.g. cascading
//! archive operations, slug generation, and single-timer invariants.
//!
//! # Modules
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`projects`] | Project lifecycle including cascade-archive |
//! | [`tasks`] | Task creation with auto-generated slugs |
//! | [`todos`] | Todo creation with project validation |
//! | [`tracker`] | Timer start/stop, duration computation, reports |
//! | [`inbox`] | Quick-capture and inbox processing |
//! | [`reminders`] | Reminder scheduling and due-check |

pub mod inbox;
pub mod projects;
pub mod reminders;
pub mod tasks;
pub mod todos;
pub mod tracker;

#[doc(inline)]
pub use inbox::InboxOps;
#[doc(inline)]
pub use projects::ProjectOps;
#[doc(inline)]
pub use reminders::ReminderOps;
#[doc(inline)]
pub use tasks::TaskOps;
#[doc(inline)]
pub use todos::TodoOps;
#[doc(inline)]
pub use tracker::TrackerOps;
