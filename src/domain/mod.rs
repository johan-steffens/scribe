// Rust guideline compliant 2026-02-21
//! Domain model: entities, repository traits, ID newtypes, and slug utilities.
//!
//! This module is the authoritative source for all business-domain types.
//! It intentionally contains no I/O — implementations live in `crate::store`.
//!
//! # Structure
//!
//! | Sub-module | Contents |
//! |---|---|
//! | [`slug`] | Slug generation and collision resolution |
//! | [`project`] | [`Project`], [`ProjectStatus`], [`Projects`] trait |
//! | [`task`] | [`Task`], [`TaskStatus`], [`TaskPriority`], [`Tasks`] trait |
//! | [`mod@todo`] | [`Todo`], [`Todos`] trait |
//! | [`time_entry`] | [`TimeEntry`], [`TimeEntries`] trait |
//! | [`capture`] | [`CaptureItem`], [`CaptureItems`] trait |
//! | [`reminder`] | [`Reminder`], [`Reminders`] trait |

pub mod capture;
pub mod project;
pub mod reminder;
pub mod slug;
pub mod task;
pub mod time_entry;
pub mod todo;

// ── newtype ID wrappers ────────────────────────────────────────────────────

/// Strongly-typed primary key for a [`project::Project`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProjectId(pub i64);

/// Strongly-typed primary key for a [`task::Task`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TaskId(pub i64);

/// Strongly-typed primary key for a [`todo::Todo`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TodoId(pub i64);

/// Strongly-typed primary key for a [`time_entry::TimeEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TimeEntryId(pub i64);

/// Strongly-typed primary key for a [`capture::CaptureItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CaptureItemId(pub i64);

/// Strongly-typed primary key for a [`reminder::Reminder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ReminderId(pub i64);

// ── inline re-exports ──────────────────────────────────────────────────────

#[doc(inline)]
pub use capture::{CaptureItem, CaptureItems, NewCaptureItem};
#[doc(inline)]
pub use project::{NewProject, Project, ProjectPatch, ProjectStatus, Projects};
#[doc(inline)]
pub use reminder::{NewReminder, Reminder, ReminderPatch, Reminders};
#[doc(inline)]
pub use task::{NewTask, Task, TaskPatch, TaskPriority, TaskStatus, Tasks};
#[doc(inline)]
pub use time_entry::{NewTimeEntry, TimeEntries, TimeEntry, TimeEntryPatch};
#[doc(inline)]
pub use todo::{NewTodo, Todo, TodoPatch, Todos};
