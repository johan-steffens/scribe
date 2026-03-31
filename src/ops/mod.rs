// Rust guideline compliant 2026-02-21
//! Business logic operations for projects and tasks.
//!
//! The `ops` layer sits between the CLI and the store layer. It enforces
//! business rules that span multiple repository calls — e.g. cascading
//! archive operations and slug generation.
//!
//! # Modules
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`projects`] | Project lifecycle including cascade-archive |
//! | [`tasks`] | Task creation with auto-generated slugs |

pub mod projects;
pub mod tasks;

#[doc(inline)]
pub use projects::ProjectOps;
#[doc(inline)]
pub use tasks::TaskOps;
