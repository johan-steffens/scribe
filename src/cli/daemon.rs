// Rust guideline compliant 2026-02-21
//! `scribe daemon` — background reminder polling loop.
//!
//! The daemon is a long-running process intended to be managed by the OS
//! service supervisor (launchd on macOS, systemd on Linux). It polls for due
//! reminders at a configurable interval and fires desktop notifications via
//! [`crate::notify`].
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

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rusqlite::Connection;

use crate::ops::ReminderOps;

// DOCUMENTED-MAGIC: 30 s default polling interval balances notification
// punctuality (reminders fire within one interval of their due time) against
// DB read frequency. At this rate the daemon issues ~2 880 queries per day,
// negligible for SQLite.
const DEFAULT_INTERVAL_SECS: u64 = 30;

/// Launches the daemon loop, blocking until the process is interrupted.
///
/// `interval_secs` controls how often the database is polled for due
/// reminders. Pass `None` to use the default (30 s).
///
/// # Errors
///
/// Returns an error only if the initial database read fails. Subsequent poll
/// errors are logged at `WARN` level and do not abort the loop.
pub fn run(conn: Arc<Mutex<Connection>>, interval_secs: Option<u64>) -> anyhow::Result<()> {
    let interval = Duration::from_secs(interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS));
    let ops = ReminderOps::new(conn);

    tracing::info!(interval_secs = interval.as_secs(), "daemon.start",);
    eprintln!(
        "Scribe daemon running — polling every {}s. Press Ctrl-C to stop.",
        interval.as_secs()
    );

    loop {
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

        thread::sleep(interval);
    }
}
