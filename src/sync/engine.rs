// Rust guideline compliant 2026-02-21
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
    pub async fn run_once(&self, local: StateSnapshot) -> Result<StateSnapshot, SyncError> {
        match self.provider.pull().await {
            Ok(remote) => {
                let remote_hash = remote.content_hash();
                let mut merged = local;
                Self::merge_into(&mut merged, &remote);
                // Only push if the merge changed anything relative to the
                // remote snapshot. This avoids unnecessary writes when local
                // and remote are already in sync.
                //
                // DOCUMENTED-MAGIC: We compare against the *remote* hash (not
                // the pre-merge local hash) because a merge that produces a
                // result identical to what the remote already has means no
                // push is necessary — the remote is already authoritative.
                if merged.content_hash() != remote_hash {
                    self.provider.push(&merged).await?;
                }
                Ok(merged)
            }
            Err(SyncError::NotFound(_)) => {
                // No remote state yet — push local as the seed.
                self.provider.push(&local).await?;
                Ok(local)
            }
            Err(e) => Err(e),
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
    // Build an index: slug → position in `local`.
    let mut local_index: HashMap<String, usize> = local
        .iter()
        .enumerate()
        .map(|(i, e)| (slug_of(e).to_owned(), i))
        .collect();

    for remote_entity in remote {
        let slug = slug_of(remote_entity).to_owned();
        if let Some(idx) = local_index.get(&slug).copied() {
            if remote_wins(remote_entity, &local[idx]) {
                local[idx] = remote_entity.clone();
            }
        } else {
            let new_idx = local.len();
            local.push(remote_entity.clone());
            local_index.insert(slug, new_idx);
        }
    }
}
