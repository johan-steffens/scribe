// Rust guideline compliant 2026-02-21
//! `scribe daemon` — background reminder polling and sync loop.
//!
//! The daemon is a long-running process intended to be managed by the OS
//! service supervisor (launchd on macOS, systemd on Linux). It polls for due
//! reminders at a configurable interval and fires desktop notifications via
//! [`crate::notify`].
//!
//! When the `sync` feature is enabled and `sync.enabled = true`, the daemon
//! also runs a sync cycle on a separate wall-clock timer, independently of the
//! reminder poll. If `sync.provider = rest` and `sync.rest.role = master`, the
//! REST sync server is started on a background thread before the loop begins.
//!
//! # Usage
//!
//! ```sh
//! scribe daemon            # poll every 30 s (default)
//! scribe daemon --interval 60
//! ```
//!
//! # Stopping
//!
//! Send `SIGINT` or `SIGTERM` (Ctrl-C) to stop gracefully.

#[cfg(feature = "sync")]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "sync")]
use chrono::Utc;
#[cfg(feature = "sync")]
use directories::ProjectDirs;
use rusqlite::Connection;
#[cfg(feature = "sync")]
use uuid::Uuid;

use crate::ops::ReminderOps;
#[cfg(feature = "sync")]
use crate::sync::{
    engine::{SyncEngine, SyncState},
    from_config,
    snapshot::StateSnapshot,
};

// DOCUMENTED-MAGIC: 30 s default polling interval balances notification
// punctuality (reminders fire within one interval of their due time) against
// DB read frequency. At this rate the daemon issues ~2 880 queries per day,
// negligible for SQLite.
const DEFAULT_INTERVAL_SECS: u64 = 30;

// DOCUMENTED-MAGIC: 1 s tick resolution means each independent timer (reminder
// poll and sync) fires within 1 s of its scheduled time. Tighter resolution
// would increase CPU wake-up frequency with no user-visible benefit.
const TICK_SECS: u64 = 1;

/// Launches the daemon loop, blocking until the process is interrupted.
///
/// `interval_secs` controls how often the database is polled for due
/// reminders. Pass `None` to use the default (30 s). When the `sync` feature
/// is enabled, `config` governs whether a sync cycle runs and whether the REST
/// master server should be started.
///
/// # Errors
///
/// Returns an error only if the initial REST server startup fails. Subsequent
/// poll and sync errors are logged at `WARN` level and do not abort the loop.
pub fn run(
    conn: &Arc<Mutex<Connection>>,
    interval_secs: Option<u64>,
    #[cfg_attr(
        not(feature = "sync"),
        expect(unused_variables, reason = "only used by sync feature")
    )]
    config: &crate::config::Config,
) -> anyhow::Result<()> {
    let reminder_interval = Duration::from_secs(interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS));

    tracing::info!(interval_secs = reminder_interval.as_secs(), "daemon.start",);
    eprintln!(
        "Scribe daemon running — polling every {}s. Press Ctrl-C to stop.",
        reminder_interval.as_secs()
    );

    // Start the REST sync master server (if configured) before entering the loop.
    #[cfg(feature = "sync")]
    spawn_rest_server_thread(conn, config)?;

    let ops = ReminderOps::new(Arc::clone(conn));

    // Pre-subtract the full interval so both timers fire immediately on
    // the first tick rather than waiting one full period.
    let mut last_reminder = Instant::now()
        .checked_sub(reminder_interval)
        .unwrap_or_else(Instant::now);

    #[cfg(feature = "sync")]
    let sync_interval = Duration::from_secs(config.sync.interval_secs);
    #[cfg(feature = "sync")]
    let sync_enabled = config.sync.enabled;
    #[cfg(feature = "sync")]
    let mut last_sync = Instant::now()
        .checked_sub(sync_interval)
        .unwrap_or_else(Instant::now);

    loop {
        let now = Instant::now();

        if now.duration_since(last_reminder) >= reminder_interval {
            match ops.check_due() {
                Ok(fired) => {
                    for reminder in &fired {
                        tracing::info!(reminder.slug = %reminder.slug, "daemon.reminder.fired");
                        crate::notify::fire(reminder);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "daemon.poll.error");
                }
            }
            last_reminder = now;
        }

        #[cfg(feature = "sync")]
        if sync_enabled && now.duration_since(last_sync) >= sync_interval {
            run_sync_cycle(conn, config);
            last_sync = now;
        }

        thread::sleep(Duration::from_secs(TICK_SECS));
    }
}

// ── sync helpers (feature-gated) ───────────────────────────────────────────

/// Runs one pull → merge → push sync cycle, logging errors rather than failing.
///
/// Errors are logged at `WARN` level and recorded in the sync-state file. This
/// function never returns an error — failures are non-fatal for the daemon loop.
#[cfg(feature = "sync")]
fn run_sync_cycle(conn: &Arc<Mutex<Connection>>, config: &crate::config::Config) {
    let provider = match from_config(config) {
        Ok(Some(p)) => p,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "sync.provider.error");
            return;
        }
    };

    // TODO(task-13): use a persisted, per-machine UUID instead of nil once
    // machine_id generation is wired into Config. machine_id is diagnostic
    // only and does not affect merge correctness.
    let local = match StateSnapshot::from_db(conn, Uuid::nil()) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "sync.snapshot.error");
            return;
        }
    };

    let sync_state_path = ProjectDirs::from("", "", "scribe").map_or_else(
        || PathBuf::from(".local/share/scribe/sync-state.json"),
        |d| d.data_dir().join("sync-state.json"),
    );

    let provider_name = config.sync.provider.to_string();
    let engine = SyncEngine::new(provider, sync_state_path.clone(), provider_name.clone());

    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "sync.runtime.error");
            return;
        }
    };

    let mut state = SyncState::load(&sync_state_path);
    match rt.block_on(engine.run_once(local)) {
        Ok(merged) => {
            if let Err(e) = merged.write_to_db(conn) {
                tracing::warn!(error = %e, "sync.write.error");
                return;
            }
            state.last_sync_at = Some(Utc::now());
            state.last_error = None;
            state.provider = Some(provider_name);
            if let Err(e) = state.save(&sync_state_path) {
                tracing::warn!(error = %e, "sync.state.save.error");
            }
            tracing::info!("sync.complete");
        }
        Err(e) => {
            tracing::warn!(error = %e, "sync.error");
            state.last_error = Some(e.to_string());
            if let Err(e) = state.save(&sync_state_path) {
                tracing::warn!(error = %e, "sync.state.save.error");
            }
        }
    }
}

/// Spawns the REST sync master server on a background thread, if configured.
///
/// Does nothing when sync is disabled, the provider is not REST, or the role
/// is not master. The spawned thread runs for the lifetime of the process.
///
/// # Errors
///
/// Returns an error if the keychain secret cannot be read or the initial
/// snapshot cannot be taken from the database.
#[cfg(feature = "sync")]
fn spawn_rest_server_thread(
    conn: &Arc<Mutex<Connection>>,
    config: &crate::config::Config,
) -> anyhow::Result<()> {
    use crate::config::{RestRole, SyncProvider};
    use crate::sync::keychain::KeychainStore;

    if !config.sync.enabled
        || config.sync.provider != SyncProvider::Rest
        || config.sync.rest.role != RestRole::Master
    {
        return Ok(());
    }

    let secret = KeychainStore::get("rest", "secret")
        .map_err(|e| anyhow::anyhow!("could not read REST secret from keychain: {e}"))?;
    let port = config.sync.rest.port;
    let initial = StateSnapshot::from_db(conn, Uuid::nil())?;

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("invariant: tokio runtime must be created");
        let (_bound_port, handle) = rt.block_on(crate::server::start_server(port, secret, initial));
        rt.block_on(handle)
            .expect("invariant: REST server task must not fail");
    });

    tracing::info!(port, "sync.rest.master.started");
    eprintln!("Scribe REST sync master listening on port {port}.");

    Ok(())
}
