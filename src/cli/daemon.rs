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
//! scribe daemon run       # same as above (explicit)
//! scribe daemon restart   # restart the daemon service
//! scribe daemon reinstall # reinstall the daemon service (after upgrades)
//! ```
//!
//! # Stopping
//!
//! Send `SIGINT` or `SIGTERM` (Ctrl-C) to stop gracefully.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "sync")]
use chrono::Utc;
#[cfg(feature = "sync")]
use directories::ProjectDirs;
use rusqlite::Connection;

use clap::Subcommand;

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

// ── clap types ─────────────────────────────────────────────────────────────

/// Subcommands for `scribe daemon`.
#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Run the background reminder notification daemon.
    ///
    /// Polls for due reminders and fires OS desktop notifications.
    /// Intended to be registered with launchd (macOS) or systemd (Linux).
    Run {
        /// Polling interval in seconds (default: 30).
        #[arg(long, short = 'i')]
        interval: Option<u64>,
    },
    /// Restart the background daemon service.
    ///
    /// This stops the currently running daemon (if any) and starts a fresh
    /// instance. Useful after config changes or when the daemon becomes
    /// unresponsive.
    Restart,
    /// Reinstall the daemon service.
    ///
    /// This uninstalls and reinstalls the service definition, picking up any
    /// changes from binary upgrades. Use this after upgrading Scribe.
    Reinstall,
}

// ── public entry point ─────────────────────────────────────────────────────

/// Executes a `scribe daemon` subcommand.
///
/// # Errors
///
/// Returns an error if the service control commands fail.
pub fn run(cmd: &DaemonCommand, config: &crate::config::Config) -> anyhow::Result<()> {
    match cmd {
        DaemonCommand::Run { interval } => {
            let conn = crate::db::open(&config.db_path())?;
            let conn = Arc::new(Mutex::new(conn));
            run_loop(&conn, *interval, config);
        }
        DaemonCommand::Restart => {
            restart_service();
        }
        DaemonCommand::Reinstall => {
            reinstall_service(config)?;
        }
    }
    Ok(())
}

// ── service control ────────────────────────────────────────────────────────

/// Restarts the daemon service by stopping and starting it.
fn restart_service() {
    #[cfg(target_os = "macos")]
    {
        let label = "com.scribe.daemon";
        println!("Restarting {label}...");

        let uid = std::env::var("UID").unwrap_or_else(|_| {
            std::process::Command::new("id")
                .args(["-u"])
                .output()
                .map_or_else(
                    |_| String::new(),
                    |o| String::from_utf8_lossy(&o.stdout).trim().to_owned(),
                )
        });

        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{label}")])
            .status();

        let _ = Command::new("launchctl")
            .args(["kickstart", "-kp", &format!("gui/{uid}/{label}")])
            .status();

        println!("{label} restarted.");
    }

    #[cfg(target_os = "linux")]
    {
        println!("Restarting scribe-daemon...");
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "scribe-daemon"])
            .status();
        println!("scribe-daemon restarted.");
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!("Restart not supported on this platform.");
    }
}

/// Reinstalls the daemon service by uninstalling and reinstalling it.
#[allow(
    clippy::too_many_lines,
    reason = "function contains platform-specific blocks that cannot be further refactored without complicating the structure"
)]
fn reinstall_service(_config: &crate::config::Config) -> anyhow::Result<()> {
    println!("Reinstalling daemon service...");

    #[cfg(target_os = "macos")]
    {
        use crate::cli::service::{LAUNCHD_LABEL, launchd_plist_path};
        let plist_path = launchd_plist_path()?;
        let _ = Command::new("launchctl")
            .args(["unload", "-w", &plist_path.to_string_lossy()])
            .status();
        if plist_path.exists() {
            std::fs::remove_file(&plist_path)?;
            println!("  Removed  {}", plist_path.display());
        }
        let binary = std::env::current_exe()?;
        let home = directories::UserDirs::new()
            .map(|u| u.home_dir().to_owned())
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{binary}</string>
    <string>daemon</string>
    <string>run</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{home}/Library/Logs/scribe-daemon.log</string>
  <key>StandardErrorPath</key>
  <string>{home}/Library/Logs/scribe-daemon.log</string>
</dict>
</plist>
"#,
            binary = binary.display(),
            home = home.display(),
        );
        if let Some(parent) = plist_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&plist_path, &plist)?;
        println!("  Created  {}", plist_path.display());
        let status = Command::new("launchctl")
            .args(["load", "-w", &plist_path.to_string_lossy()])
            .status()?;
        if status.success() {
            println!("  Started  {LAUNCHD_LABEL} (launchctl)");
        }
        println!();
        println!("Daemon service reinstalled.");
    };

    #[cfg(target_os = "linux")]
    {
        use crate::cli::service::systemd_unit_path;
        let unit_path = systemd_unit_path()?;

        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", "scribe-daemon"])
            .status();

        if unit_path.exists() {
            std::fs::remove_file(&unit_path)?;
            println!("  Removed  {}", unit_path.display());
        }

        let binary = std::env::current_exe()?;
        let unit = format!(
            "[Unit]\n\
             Description=Scribe reminder notification daemon\n\
             After=graphical-session.target\n\
             \n\
             [Service]\n\
             ExecStart={binary}\n\
             Restart=on-failure\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n",
            binary = binary.display(),
        );

        if let Some(parent) = unit_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&unit_path, &unit)?;
        println!("  Created  {}", unit_path.display());

        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status()?;

        let _ = Command::new("systemctl")
            .args(["--user", "enable", "--now", "scribe-daemon"])
            .status()?;

        println!();
        println!("Daemon service reinstalled.");
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!("Daemon reinstall not supported on this platform.");
    };

    Ok(())
}

/// Runs the daemon loop, blocking until the process is interrupted.
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
fn run_loop(
    conn: &Arc<Mutex<Connection>>,
    interval_secs: Option<u64>,
    #[cfg_attr(
        not(feature = "sync"),
        expect(unused_variables, reason = "only used by sync feature")
    )]
    config: &crate::config::Config,
) {
    let reminder_interval = Duration::from_secs(interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS));

    tracing::info!(interval_secs = reminder_interval.as_secs(), "daemon.start",);
    eprintln!(
        "Scribe daemon running — polling every {}s. Press Ctrl-C to stop.",
        reminder_interval.as_secs()
    );

    #[cfg(feature = "sync")]
    run_bootstrap();

    // Start the REST sync master server (if configured) before entering the loop.
    #[cfg(feature = "sync")]
    if let Err(e) = spawn_rest_server_thread(conn, config) {
        tracing::warn!(error = %e, "daemon: failed to start REST server");
    };

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
        #[cfg(feature = "sync")]
        run_bootstrap();

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

/// Checks for and applies a keychain bootstrap file.
///
/// This bridges the "Two Vaults" problem on macOS where the CLI (running in a
/// user session) and the daemon (running in a launchd session) have different
/// code signatures or path identities, preventing the daemon from reading
/// secrets the CLI just wrote. The CLI writes a transient JSON file which the
/// daemon consumes to populate its own view of the keychain.
#[cfg(feature = "sync")]
fn run_bootstrap() {
    crate::sync::keychain::KeychainStore::apply_bootstrap();
}

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

    let local = match StateSnapshot::from_db(conn, config.machine_id()) {
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
    use crate::server::ServerConfig;
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
    let initial = StateSnapshot::from_db(conn, config.machine_id())?;
    let server_config = ServerConfig {
        db_path: config.db_path(),
        machine_id: config.machine_id(),
        refresh_interval_secs: config.sync.interval_secs,
    };

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("invariant: tokio runtime must be created");
        let (_bound_port, handle) = rt.block_on(crate::server::start_server(
            port,
            secret,
            initial,
            server_config,
        ));
        rt.block_on(handle)
            .expect("invariant: REST server task must not fail");
    });

    tracing::info!(port, "sync.rest.master.started");
    eprintln!("Scribe REST sync master listening on port {port}.");

    Ok(())
}
