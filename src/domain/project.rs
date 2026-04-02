// Rust guideline compliant 2026-02-21
//! `Project` entity, status enum, and the `Projects` repository trait.
//!
//! Projects are the top-level organisational unit. Every task, todo, time
//! entry, and reminder belongs to exactly one project. The reserved
//! `quick-capture` project (`is_reserved = true`) cannot be deleted or
//! archived and acts as the default inbox.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::ProjectId;

// ── status enum ────────────────────────────────────────────────────────────

/// Lifecycle status of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    /// Project is actively worked on.
    Active,
    /// Project is temporarily paused.
    Paused,
    /// Project has been completed.
    Completed,
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
        }
    }
}

impl std::str::FromStr for ProjectStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            other => Err(format!("unknown project status '{other}'")),
        }
    }
}

// ── entity struct ──────────────────────────────────────────────────────────

/// A project record as stored in the database.
///
/// Projects own tasks, todos, time entries, and reminders. The `slug` field
/// is the user-facing identifier used in the CLI and TUI. The numeric `id` is
/// internal only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Internal numeric primary key (not exposed to users).
    pub id: ProjectId,
    /// Unique kebab-case identifier, e.g. `payment-automation`.
    pub slug: String,
    /// Human-readable name, e.g. `"Payment Automation"`.
    pub name: String,
    /// Optional free-text description.
    pub description: Option<String>,
    /// Current lifecycle status.
    pub status: ProjectStatus,
    /// When `true`, the project cannot be deleted or archived.
    pub is_reserved: bool,
    /// Timestamp when archived; `None` means the project is active.
    pub archived_at: Option<DateTime<Utc>>,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Last-updated timestamp (UTC).
    pub updated_at: DateTime<Utc>,
}

// ── repository trait ───────────────────────────────────────────────────────

/// Data-access operations for the `projects` table.
///
/// Implementations must never return raw `rusqlite` types through this trait;
/// all errors must be mapped to `anyhow::Error` or a concrete domain error.
pub trait Projects {
    /// Inserts a new project and returns the persisted record.
    ///
    /// # Errors
    ///
    /// Returns an error if the slug already exists or if a database error
    /// occurs.
    fn create(&self, project: NewProject) -> anyhow::Result<Project>;

    /// Looks up a project by its user-facing slug.
    ///
    /// Returns `Ok(None)` when no project with that slug exists.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Project>>;

    /// Returns all active (non-archived) projects.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    // Used in Phase 2+ direct listing; use `list()` in Phase 1 code.
    #[allow(dead_code, reason = "used in Phase 2+ direct listing paths")]
    fn list_active(&self) -> anyhow::Result<Vec<Project>>;

    /// Returns all archived projects.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    // Reserved for Phase 2+ archive browsing.
    #[allow(dead_code, reason = "used in Phase 2+ archive browsing")]
    fn list_archived(&self) -> anyhow::Result<Vec<Project>>;

    /// Lists projects filtered by status, including archived when requested.
    ///
    /// When `include_archived` is `true` both active and archived rows are
    /// returned.
    ///
    /// # Errors
    ///
    /// Returns an error on database failure.
    fn list(
        &self,
        status: Option<ProjectStatus>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Project>>;

    /// Updates mutable fields of an existing project.
    ///
    /// Only fields wrapped in `Some` are updated; `None` fields are left
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist or a database error
    /// occurs.
    fn update(&self, slug: &str, patch: ProjectPatch) -> anyhow::Result<Project>;

    /// Archives the project identified by `slug`.
    ///
    /// Sets `archived_at` to the current UTC time. Blocked on reserved
    /// projects.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is reserved, does not exist, or a
    /// database error occurs.
    fn archive(&self, slug: &str) -> anyhow::Result<Project>;

    /// Restores an archived project, clearing its `archived_at` timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist or a database error
    /// occurs.
    fn restore(&self, slug: &str) -> anyhow::Result<Project>;

    /// Permanently deletes the project row from the database.
    ///
    /// Blocked on reserved projects and on projects that still have linked
    /// items (enforced by `ON DELETE RESTRICT`).
    ///
    /// # Errors
    ///
    /// Returns an error if the project is reserved, has linked items, does not
    /// exist, or a database error occurs.
    fn delete(&self, slug: &str) -> anyhow::Result<()>;
}

// ── input types ────────────────────────────────────────────────────────────

/// Parameters required to create a new project.
#[derive(Debug, Clone)]
pub struct NewProject {
    /// Unique kebab-case slug chosen by the user.
    pub slug: String,
    /// Human-readable project name.
    pub name: String,
    /// Optional free-text description.
    pub description: Option<String>,
    /// Initial lifecycle status (defaults to `Active`).
    pub status: ProjectStatus,
}

/// Partial update for mutable project fields.
///
/// `None` values are not written; only `Some` values trigger an update.
#[derive(Debug, Clone, Default)]
pub struct ProjectPatch {
    /// New slug, if changing.
    pub slug: Option<String>,
    /// New name, if changing.
    pub name: Option<String>,
    /// New description override: `Some(s)` sets, `None` leaves unchanged.
    ///
    /// To clear the description, pass `Some("")` — the store normalises
    /// empty strings to SQL `NULL`.
    pub description: Option<String>,
    /// Whether to clear the description entirely.
    pub clear_description: bool,
    /// New status, if changing.
    pub status: Option<ProjectStatus>,
}
