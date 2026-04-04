//! `SyncEngine` — pull → merge → push cycle with last-write-wins semantics.
//!
//! The engine drives a single sync round-trip:
//!
//! 1. **Pull** the remote [`StateSnapshot`] from a [`SyncProvider`].
//! 2. **Merge** the remote entities into the local snapshot using
//!    last-write-wins conflict resolution (keyed on `slug`).
//! 3. **Push** the merged result back to the provider, unless the content hash
//!    is unchanged (idempotent push avoidance).
//!
//! # Merge rules
//!
//! | Entity type | Conflict resolution |
//! |---|---|
//! | `Project`, `Task`, `Todo` | Remote `updated_at` > local → replace; otherwise keep local |
//! | `TimeEntry`, `Reminder`, `CaptureItem` | Insert-or-keep (no `updated_at`; never replace) |
//!
//! # Persistence
//!
//! [`SyncState`] records metadata about the last sync attempt (timestamp,
//! error, provider name) in a JSON file at `~/.local/share/scribe/sync-state.json`.
//! It is stored outside `SQLite` deliberately so that the sync metadata never
//! travels with the synced dataset.

// DOCUMENTED-MAGIC: Engine items unused until Tasks 12/13 wire StateSnapshot DB methods.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{CaptureItem, Project, Reminder, Task, TimeEntry, Todo};
use crate::sync::{StateSnapshot, SyncError, SyncProvider};

// ── SyncState ──────────────────────────────────────────────────────────────

/// Persisted record of the last sync attempt.
///
/// Stored at `~/.local/share/scribe/sync-state.json`. Not in `SQLite` — keeps
/// sync metadata out of the synced dataset.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    /// UTC timestamp of the most recently completed sync, if any.
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Human-readable description of the last error, if any.
    pub last_error: Option<String>,
    /// Name of the provider used for the last sync, if any.
    pub provider: Option<String>,
}

impl SyncState {
    /// Loads from disk, returning a default if absent or unparseable.
    ///
    /// A missing file is treated as "never synced" rather than an error,
    /// matching first-run behaviour. Unparseable files are also silently
    /// replaced with the default to avoid blocking the user on a corrupt state.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persists to disk, creating parent dirs as needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created, the file
    /// cannot be written, or serialisation fails.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

// ── SyncSummary ─────────────────────────────────────────────────────────────

/// Summary of a single sync operation, tracking what was pulled from and
/// pushed to the remote.
///
/// `pulled_additions` counts entities that existed only on the remote side
/// and were added to the local snapshot during merge. `pulled_updates` counts
/// entities that existed on both sides but were replaced because the remote
/// version was newer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncSummary {
    pub projects_added: u32,
    pub projects_updated: u32,
    pub tasks_added: u32,
    pub tasks_updated: u32,
    pub todos_added: u32,
    pub todos_updated: u32,
    pub time_entries_added: u32,
    pub time_entries_updated: u32,
    pub reminders_added: u32,
    pub reminders_updated: u32,
    pub capture_items_added: u32,
    pub capture_items_updated: u32,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl SyncSummary {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_error(error: String) -> Self {
        Self {
            error: Some(error),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn total_pulled(&self) -> u32 {
        self.projects_added
            + self.projects_updated
            + self.tasks_added
            + self.tasks_updated
            + self.todos_added
            + self.todos_updated
            + self.time_entries_added
            + self.time_entries_updated
            + self.reminders_added
            + self.reminders_updated
            + self.capture_items_added
            + self.capture_items_updated
    }

    #[must_use]
    pub fn from_comparison(original: &StateSnapshot, merged: &StateSnapshot) -> Self {
        let (projects_added, projects_updated) = diff_counts(&original.projects, &merged.projects);
        let (tasks_added, tasks_updated) = diff_counts(&original.tasks, &merged.tasks);
        let (todos_added, todos_updated) = diff_counts(&original.todos, &merged.todos);
        let (time_entries_added, time_entries_updated) =
            diff_counts(&original.time_entries, &merged.time_entries);
        let (reminders_added, reminders_updated) =
            diff_counts(&original.reminders, &merged.reminders);
        let (capture_items_added, capture_items_updated) =
            diff_counts(&original.capture_items, &merged.capture_items);

        Self {
            projects_added,
            projects_updated,
            tasks_added,
            tasks_updated,
            todos_added,
            todos_updated,
            time_entries_added,
            time_entries_updated,
            reminders_added,
            reminders_updated,
            capture_items_added,
            capture_items_updated,
            last_sync_at: Some(Utc::now()),
            error: None,
        }
    }
}

fn diff_counts<T: Clone + serde::Serialize>(original: &[T], merged: &[T]) -> (u32, u32) {
    let original_slugs: Vec<_> = original
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect();
    let merged_slugs: Vec<_> = merged
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect();
    let added = count_added(&original_slugs, &merged_slugs);
    let updated = count_updated(&original_slugs, &merged_slugs);
    (added, updated)
}

fn count_added(original: &[String], merged: &[String]) -> u32 {
    let original_keys: std::collections::HashSet<_> = original.iter().collect();
    let merged_keys: std::collections::HashSet<_> = merged.iter().collect();
    u32::try_from(merged_keys.difference(&original_keys).count()).unwrap_or(u32::MAX)
}

fn count_updated(original: &[String], merged: &[String]) -> u32 {
    let mut count = 0u32;
    let original_map: std::collections::HashMap<_, _> =
        original.iter().map(|s| (s.as_str(), s)).collect();
    for merged_entity in merged {
        if let Some(orig) = original_map.get(merged_entity.as_str())
            && **orig != **merged_entity
        {
            count += 1;
        }
    }
    count
}

// ── SyncEngine ─────────────────────────────────────────────────────────────

/// Orchestrates pull → merge → push sync cycles against a [`SyncProvider`].
pub struct SyncEngine {
    provider: Box<dyn SyncProvider>,
    sync_state_path: PathBuf,
    provider_name: String,
}

impl fmt::Debug for SyncEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncEngine")
            .field("provider_name", &self.provider_name)
            .finish_non_exhaustive()
        // DOCUMENTED-MAGIC: finish_non_exhaustive omits `provider` and
        // `sync_state_path` from debug output since they are either a
        // trait object (unprintable) or a potentially long path.
    }
}

impl SyncEngine {
    /// Creates a new engine backed by the given provider.
    #[must_use]
    pub fn new(
        provider: Box<dyn SyncProvider>,
        sync_state_path: PathBuf,
        provider_name: String,
    ) -> Self {
        Self {
            provider,
            sync_state_path,
            provider_name,
        }
    }

    /// Runs one pull → merge → push cycle.
    ///
    /// On [`SyncError::NotFound`] from pull (no remote state yet), pushes local
    /// as the initial snapshot. Skips push if content hash is unchanged after merge.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the pull or push operation fails for any
    /// reason other than `NotFound` on pull.
    pub async fn run_once(
        &self,
        local: StateSnapshot,
    ) -> Result<(StateSnapshot, SyncSummary), SyncError> {
        let original = local.clone();
        let local_hash = local.content_hash();
        tracing::info!(
            provider = %self.provider_name,
            local_entities = local.entities(),
            local_hash = %local_hash,
            "sync: starting pull phase"
        );

        match self.provider.pull().await {
            Ok(remote) => {
                let remote_hash = remote.content_hash();
                tracing::info!(
                    remote_entities = remote.entities(),
                    remote_hash = %remote_hash,
                    "sync: pull succeeded, merging"
                );
                let mut merged = local;
                tracing::debug!(
                    before_merge_entities = merged.entities(),
                    "sync: before merge"
                );
                Self::merge_into(&mut merged, &remote);
                tracing::debug!(
                    after_merge_entities = merged.entities(),
                    "sync: after merge"
                );
                let merged_hash = merged.content_hash();
                if merged_hash == remote_hash {
                    tracing::info!("sync: content unchanged, skipping push");
                } else {
                    tracing::info!(
                        merged_entities = merged.entities(),
                        merged_hash = %merged_hash,
                        remote_hash = %remote_hash,
                        "sync: content changed, pushing"
                    );
                    self.provider.push(&merged).await?;
                    tracing::info!("sync: push succeeded");
                }
                let summary = SyncSummary::from_comparison(&original, &merged);
                Ok((merged, summary))
            }
            Err(SyncError::NotFound(_)) => {
                tracing::info!(
                    local_hash = %local_hash,
                    "sync: no remote state, pushing initial snapshot"
                );
                self.provider.push(&local).await?;
                tracing::info!("sync: initial push succeeded");
                let summary = SyncSummary::from_comparison(&original, &local);
                Ok((local, summary))
            }
            Err(e) => {
                tracing::warn!(error = %e, "sync: pull failed");
                Err(e)
            }
        }
    }

    /// Merges remote entities into local snapshot in place.
    ///
    /// Merge rules (keyed by `slug`):
    /// - Remote-only entities: inserted into local.
    /// - Both exist, remote `updated_at` > local: remote replaces local.
    /// - Both exist, local `updated_at` >= remote: local is preserved.
    /// - `TimeEntry`, `Reminder`, `CaptureItem` (no `updated_at`): insert-or-keep only.
    pub fn merge_into(local: &mut StateSnapshot, remote: &StateSnapshot) {
        merge_entities(
            &mut local.projects,
            &remote.projects,
            |p| &p.slug,
            |rem, loc| rem.updated_at > loc.updated_at,
        );
        merge_entities(
            &mut local.tasks,
            &remote.tasks,
            |t| &t.slug,
            |rem, loc| rem.updated_at > loc.updated_at,
        );
        merge_entities(
            &mut local.todos,
            &remote.todos,
            |t| &t.slug,
            |rem, loc| rem.updated_at > loc.updated_at,
        );
        // TimeEntry, Reminder, CaptureItem have no `updated_at` field —
        // use insert-or-keep semantics (remote_wins always returns false).
        merge_entities(
            &mut local.time_entries,
            &remote.time_entries,
            |e| &e.slug,
            |_rem, _loc| false,
        );
        merge_entities(
            &mut local.reminders,
            &remote.reminders,
            |r| &r.slug,
            |_rem, _loc| false,
        );
        merge_entities(
            &mut local.capture_items,
            &remote.capture_items,
            |c| &c.slug,
            |_rem, _loc| false,
        );
    }

    /// Returns the path where sync state is persisted.
    #[must_use]
    pub fn sync_state_path(&self) -> &Path {
        &self.sync_state_path
    }

    /// Returns the name of the configured provider.
    #[must_use]
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }
}

// ── private merge helper ───────────────────────────────────────────────────

/// Merges `remote` entities into `local` using slug-keyed conflict resolution.
///
/// - Remote-only slugs are appended to `local`.
/// - Conflicting slugs: `remote_wins(remote_entity, local_entity)` decides.
///   When it returns `true`, the local entry is replaced; otherwise kept.
fn merge_entities<T: Clone>(
    local: &mut Vec<T>,
    remote: &[T],
    slug_of: impl Fn(&T) -> &str,
    remote_wins: impl Fn(&T, &T) -> bool,
) {
    let initial_local_len = local.len();
    tracing::debug!(
        initial_local_len,
        remote_len = remote.len(),
        "merge: starting"
    );

    // Build an index: slug → position in `local`.
    let mut local_index: HashMap<String, usize> = local
        .iter()
        .enumerate()
        .map(|(i, e)| (slug_of(e).to_owned(), i))
        .collect();

    let mut added_count = 0;
    let mut replaced_count = 0;

    for remote_entity in remote {
        let slug = slug_of(remote_entity).to_owned();
        if let Some(idx) = local_index.get(&slug).copied() {
            if remote_wins(remote_entity, &local[idx]) {
                local[idx] = remote_entity.clone();
                replaced_count += 1;
            }
        } else {
            let new_idx = local.len();
            local.push(remote_entity.clone());
            local_index.insert(slug, new_idx);
            added_count += 1;
        }
    }

    tracing::debug!(
        initial_local_len,
        final_local_len = local.len(),
        added_count,
        replaced_count,
        "merge: complete"
    );
}
