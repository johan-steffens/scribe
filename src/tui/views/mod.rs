//! TUI view modules — one per navigable screen.
//!
//! Each view is a pure rendering function that accepts `&App` and writes to a
//! [`ratatui::Frame`]. No view holds mutable state.
//!
//! # Views
//!
//! | Module | View |
//! |---|---|
//! | [`dashboard`] | Home: today's tasks + active timer |
//! | [`projects`] | Full project list with live filter and CRUD |
//! | [`tasks`] | Full task list with live filter and CRUD |
//! | [`todos`] | Todo list with done-toggle and CRUD |
//! | [`tracker`] | Time-entry history and active timer |
//! | [`inbox`] | Quick-capture inbox with process dialog |
//! | [`reminders`] | Active reminders with CRUD |

pub mod dashboard;
pub mod inbox;
pub mod placeholder;
pub mod projects;
pub mod reminders;
pub mod tasks;
pub mod todos;
pub mod tracker;
