//! `scribe service` — background daemon service management.
//!
//! Installs and uninstalls the `scribe daemon` as a system-managed background
//! service so that reminder notifications are delivered automatically at login
//! without any manual intervention.
//!
//! # Platform support
//!
//! | Platform | Mechanism | Unit/file written |
//! |---|---|---|
//! | macOS | launchd user agent | `~/Library/LaunchAgents/com.scribe.daemon.plist` |
//! | Linux | systemd user unit | `~/.config/systemd/user/scribe-daemon.service` |
//! | Other | — | prints a manual-start suggestion |
//!
//! # Usage
//!
//! ```sh
//! scribe service run        # run the daemon directly
//! scribe service install    # install and start
//! scribe service uninstall  # stop and remove
//! scribe service restart   # restart the service
//! scribe service reinstall # reinstall the service
//! scribe service status     # print current status
//! ```

use std::path::{Path, PathBuf};
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

use crate::config::Config;
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

/// Subcommands for `scribe service`.
#[derive(Debug, Subcommand)]
pub enum ServiceCommand {
    /// Run the background reminder notification daemon directly.
    ///
    /// Polls for due reminders and fires OS desktop notifications.
    Run {
        /// Polling interval in seconds (default: 30).
        #[arg(long, short = 'i')]
        interval: Option<u64>,
    },
    /// Install and start the background reminder daemon service.
    Install,
    /// Stop and remove the background reminder daemon service.
    Uninstall,
    /// Show whether the background daemon service is currently installed.
    Status,
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

/// Executes a `scribe service` subcommand.
///
/// `config` is loaded by the caller and passed in so that install/uninstall
/// can update the `[setup]` section and save it.
///
/// # Errors
///
/// Returns an error if the home directory cannot be found, a file write
/// fails, or a `launchctl`/`systemctl` call exits with a non-zero status.
pub fn run(
    cmd: &ServiceCommand,
    config: &mut Config,
    conn: Option<&Arc<Mutex<Connection>>>,
) -> anyhow::Result<()> {
    match cmd {
        ServiceCommand::Run { interval } => {
            let db = conn.ok_or_else(|| anyhow::anyhow!("database connection required"))?;
            run_loop(db, *interval, config)
        }
        ServiceCommand::Install => install(config),
        ServiceCommand::Uninstall => uninstall(config),
        ServiceCommand::Status => {
            status(config);
            Ok(())
        }
        ServiceCommand::Restart => {
            restart_service();
            Ok(())
        }
        ServiceCommand::Reinstall => reinstall_service(config),
    }
}

// ── install ────────────────────────────────────────────────────────────────

/// Installs the daemon service for the current platform.
fn install(config: &mut Config) -> anyhow::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if config.setup.daemon_service_installed {
            match validate_service_file() {
                Ok(true) => {
                    println!(
                        "Daemon service is already installed with correct configuration. Run `scribe service status` for details."
                    );
                    return Ok(());
                }
                Ok(false) => {
                    println!(
                        "Daemon service file exists but has incorrect binary path. Repairing..."
                    );
                    repair_service(config)?;
                    return Ok(());
                }
                Err(e) => {
                    println!("Could not validate existing service file: {e}. Reinstalling...");
                    repair_service(config)?;
                    return Ok(());
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    install_launchd(config)?;

    #[cfg(target_os = "linux")]
    install_systemd(config)?;

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!(
            "Automatic service installation is not supported on this platform.\n\
             \n\
             To run the background daemon:\n\
             - Option 1: Run 'scribe service run' in a terminal (foreground)\n\
             - Option 2: Use Windows Task Scheduler to run 'scribe service run' at startup\n\
             - Option 3: Use NSSM (nssm.org) to install as a Windows service\n\
             \n\
             The daemon polls for reminders every {DEFAULT_INTERVAL_SECS}s."
        );
    }

    Ok(())
}

/// Validates that the existing service file points to the current binary.
///
/// Returns `Ok(true)` if the file exists and has the correct binary path,
/// `Ok(false)` if the file does not exist or has an incorrect path, and `Err`
/// if the file cannot be read or parsed.
fn validate_service_file() -> anyhow::Result<bool> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = launchd_plist_path()?;
        if !plist_path.exists() {
            return Ok(false);
        }
        let content = std::fs::read_to_string(&plist_path)?;
        let current_binary = current_binary_path()?;
        let expected_path = current_binary.to_string_lossy();
        if let Some(first_arg) = parse_plist_program_arguments(&content) {
            Ok(first_arg == *expected_path)
        } else {
            Ok(false)
        }
    }

    #[cfg(target_os = "linux")]
    {
        let unit_path = systemd_unit_path()?;
        if !unit_path.exists() {
            return Ok(false);
        }
        let content = std::fs::read_to_string(&unit_path)?;
        let current_binary = current_binary_path()?;
        let expected_prefix = current_binary.to_string_lossy().into_owned();
        if let Some(exec_start) = parse_systemd_exec_start(&content) {
            Ok(exec_start.starts_with(&expected_prefix) && exec_start.contains(" service run"))
        } else {
            Ok(false)
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(true)
    }
}

/// Parses a launchd plist and returns the first element of `ProgramArguments`.
#[cfg(target_os = "macos")]
fn parse_plist_program_arguments(content: &str) -> Option<String> {
    let mut in_program_arguments = false;
    let mut array_depth = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.contains("ProgramArguments") {
            in_program_arguments = true;
            continue;
        }
        if in_program_arguments {
            if trimmed.starts_with("<array>") {
                array_depth += 1;
                continue;
            }
            if trimmed.starts_with("</array>") {
                break;
            }
            if array_depth > 0
                && trimmed.starts_with("<string>")
                && let Some(end) = trimmed.find("</string>")
            {
                let start = trimmed.find('<').unwrap();
                return Some(trimmed[start + 8..end].to_string());
            }
        }
    }
    None
}

/// Parses a systemd unit file and returns the `ExecStart` value.
#[cfg(target_os = "linux")]
fn parse_systemd_exec_start(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("ExecStart=") {
            let value = trimmed.strip_prefix("ExecStart=").unwrap().trim();
            return Some(value.to_string());
        }
    }
    None
}

/// Repairs the service file by regenerating it with the correct binary path.
fn repair_service(config: &mut Config) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = launchd_plist_path()?;
        let binary = current_binary_path()?;
        let home = home_dir()?;
        let plist = generate_launchd_plist(&binary, &home);
        std::fs::write(&plist_path, &plist)?;
        println!("  Repaired {}", plist_path.display());
        let status = Command::new("launchctl")
            .args(["load", "-w", &plist_path.to_string_lossy()])
            .status()?;
        if !status.success() {
            anyhow::bail!(
                "launchctl load failed (exit {}). The plist was written but the \
                 agent was not loaded.",
                status.code().unwrap_or(-1),
            );
        }
        println!("  Started  {LAUNCHD_LABEL} (launchctl)");
        config.setup.daemon_service_installed = true;
        config.save()?;
    };

    #[cfg(target_os = "linux")]
    {
        let unit_path = systemd_unit_path()?;
        let binary = current_binary_path()?;
        let unit = generate_systemd_unit(&binary);
        std::fs::write(&unit_path, &unit)?;
        println!("  Repaired {}", unit_path.display());
        let reload = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status()?;
        if !reload.success() {
            anyhow::bail!("systemctl --user daemon-reload failed");
        }
        let enable = Command::new("systemctl")
            .args(["--user", "enable", "--now", "scribe-daemon"])
            .status()?;
        if !enable.success() {
            anyhow::bail!(
                "systemctl --user enable --now failed. The unit file was written \
                 but the service was not started.",
            );
        }
        config.setup.daemon_service_installed = true;
        config.save()?;
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!(
            "Service repair is not supported on this platform.\n\
             On Windows, manually reinstall using Task Scheduler or NSSM."
        );
    };

    Ok(())
}

// ── uninstall ──────────────────────────────────────────────────────────────

/// Uninstalls the daemon service for the current platform.
fn uninstall(config: &mut Config) -> anyhow::Result<()> {
    if !config.setup.daemon_service_installed {
        println!("Daemon service is not installed.");
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    uninstall_launchd(config)?;

    #[cfg(target_os = "linux")]
    uninstall_systemd(config)?;

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!(
            "Automatic service removal is not supported on this platform.\n\
             On Windows, use Task Scheduler or NSSM to remove the scheduled task/service."
        );
    }

    Ok(())
}

// ── service state checker ───────────────────────────────────────────────────

/// Represents the actual state of the daemon service on the system.
pub(crate) struct ServiceState {
    pub config_says_installed: bool,
    pub file_exists: bool,
    pub is_running: bool,
}

impl ServiceState {
    pub fn check(config: &Config) -> Self {
        let config_says_installed = config.setup.daemon_service_installed;

        #[cfg(target_os = "macos")]
        let file_exists = launchd_plist_path().map(|p| p.exists()).unwrap_or(false);
        #[cfg(target_os = "linux")]
        let file_exists = systemd_unit_path().map(|p| p.exists()).unwrap_or(false);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let file_exists = false;

        let is_running = check_daemon_running();

        Self {
            config_says_installed,
            file_exists,
            is_running,
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.config_says_installed == self.file_exists
            && self.config_says_installed == self.is_running
    }

    pub fn has_discrepancy(&self) -> bool {
        self.config_says_installed != self.file_exists
    }
}

fn check_daemon_running() -> bool {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("launchctl").args(["list"]).output().ok();
        output.is_some_and(|o| String::from_utf8_lossy(&o.stdout).contains("com.scribe.daemon"))
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", "scribe-daemon"])
            .output()
            .ok();
        output.is_some_and(|o| {
            let status = String::from_utf8_lossy(&o.stdout);
            status.trim() == "active"
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

// ── status ─────────────────────────────────────────────────────────────────

/// Prints the current service installation status.
pub fn status(config: &Config) {
    let state = ServiceState::check(config);

    println!("Daemon service diagnostic");
    println!("=========================");
    println!();
    println!(
        "  {:<20} {:>10}",
        "Config flag:",
        if state.config_says_installed {
            "installed"
        } else {
            "not installed"
        }
    );

    #[cfg(target_os = "macos")]
    let file_path = launchd_plist_path()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    #[cfg(target_os = "linux")]
    let file_path = systemd_unit_path()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let file_path = String::new();

    println!(
        "  {:<20} {:>10}",
        "Service file:",
        if state.file_exists {
            "present"
        } else {
            "missing"
        }
    );
    if !file_path.is_empty() {
        println!("  {:<20} {}", "", file_path);
    }
    println!(
        "  {:<20} {:>10}",
        "Process running:",
        if state.is_running { "yes" } else { "no" }
    );

    println!();

    if state.config_says_installed && !state.file_exists {
        println!("  ⚠ BROKEN: Config says installed but service file is missing.");
        println!("  Run `scribe service reinstall` to repair.");
    } else if !state.config_says_installed && state.file_exists {
        println!("  ⚠ ORPHANED: Service file exists but config says not installed.");
        println!("  Run `scribe service uninstall` to clean up.");
    } else if state.config_says_installed && !state.is_running {
        println!("  ⚠ STOPPED: Service is installed but not running.");
        println!("  Run `scribe service restart` to start it.");
    } else if state.is_healthy() {
        println!("  ✓ All checks passed.");
    } else {
        println!("  ✓ Not installed (expected state).");
    }

    println!();
}

// ── macOS / launchd ────────────────────────────────────────────────────────

// DOCUMENTED-MAGIC: The label matches the plist filename by convention so
// that launchctl load/unload and status lookups are consistent.
#[cfg(target_os = "macos")]
pub(crate) const LAUNCHD_LABEL: &str = "com.scribe.daemon";

/// Returns `~/Library/LaunchAgents/com.scribe.daemon.plist`.
#[cfg(target_os = "macos")]
pub(crate) fn launchd_plist_path() -> anyhow::Result<PathBuf> {
    let home = home_dir()?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn generate_launchd_plist(binary: &Path, home: &Path) -> String {
    format!(
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
    <string>service</string>
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
    )
}

#[cfg(target_os = "macos")]
fn install_launchd(config: &mut Config) -> anyhow::Result<()> {
    let binary = current_binary_path()?;
    let plist_path = launchd_plist_path()?;
    let home = home_dir()?;

    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let plist = generate_launchd_plist(&binary, &home);
    std::fs::write(&plist_path, plist)?;
    println!("  Created  {}", plist_path.display());

    let status = Command::new("launchctl")
        .args(["load", "-w", &plist_path.to_string_lossy()])
        .status()?;

    if status.success() {
        println!("  Started  {LAUNCHD_LABEL} (launchctl)");
    } else {
        anyhow::bail!(
            "launchctl load failed (exit {}). The plist was written but the \
             agent was not loaded. You can load it manually with:\n\
             \n  launchctl load -w {}",
            status.code().unwrap_or(-1),
            plist_path.display(),
        );
    }

    config.setup.daemon_service_installed = true;
    config.save()?;

    println!();
    println!("Daemon service installed. Scribe will now deliver reminder");
    println!("notifications automatically at login and while you work.");
    println!();
    println!("Notifications appear in Notification Centre attributed to");
    println!("Script Editor. Enable them in:");
    println!("  System Settings → Notifications → Script Editor");

    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd(config: &mut Config) -> anyhow::Result<()> {
    let plist_path = launchd_plist_path()?;

    // Unload (stop) the agent first — ignore errors if it isn't loaded.
    let _ = Command::new("launchctl")
        .args(["unload", "-w", &plist_path.to_string_lossy()])
        .status();

    if plist_path.exists() {
        std::fs::remove_file(&plist_path)?;
        println!("  Removed  {}", plist_path.display());
    }

    println!("  Stopped  {LAUNCHD_LABEL} (launchctl)");

    config.setup.daemon_service_installed = false;
    config.save()?;

    println!();
    println!("Daemon service uninstalled. Background notifications are disabled.");

    Ok(())
}

// ── Linux / systemd ────────────────────────────────────────────────────────

/// Returns `~/.config/systemd/user/scribe-daemon.service`.
#[cfg(target_os = "linux")]
pub(crate) fn systemd_unit_path() -> anyhow::Result<PathBuf> {
    let home = home_dir()?;
    Ok(home
        .join(".config")
        .join("systemd")
        .join("user")
        .join("scribe-daemon.service"))
}

#[cfg(target_os = "linux")]
fn generate_systemd_unit(binary: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=Scribe reminder notification daemon\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         ExecStart={binary} service run\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        binary = binary.display(),
    )
}

#[cfg(target_os = "linux")]
fn install_systemd(config: &mut Config) -> anyhow::Result<()> {
    let binary = current_binary_path()?;
    let unit_path = systemd_unit_path()?;

    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let unit = generate_systemd_unit(&binary);
    std::fs::write(&unit_path, unit)?;
    println!("  Created  {}", unit_path.display());

    let reload = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()?;

    if !reload.success() {
        anyhow::bail!("systemctl --user daemon-reload failed");
    }

    let enable = Command::new("systemctl")
        .args(["--user", "enable", "--now", "scribe-daemon"])
        .status()?;

    if enable.success() {
        println!("  Enabled  scribe-daemon (systemctl --user)");
    } else {
        anyhow::bail!(
            "systemctl --user enable --now failed. The unit file was written \
             but the service was not started. Enable it manually with:\n\
             \n  systemctl --user enable --now scribe-daemon"
        );
    }

    config.setup.daemon_service_installed = true;
    config.save()?;

    println!();
    println!("Daemon service installed. Scribe will now deliver reminder");
    println!("notifications automatically.");

    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_systemd(config: &mut Config) -> anyhow::Result<()> {
    let unit_path = systemd_unit_path()?;

    // Stop and disable — ignore errors if not loaded.
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "scribe-daemon"])
        .status();

    if unit_path.exists() {
        std::fs::remove_file(&unit_path)?;
        println!("  Removed  {}", unit_path.display());
    }

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    println!("  Stopped  scribe-daemon (systemctl --user)");

    config.setup.daemon_service_installed = false;
    config.save()?;

    println!();
    println!("Daemon service uninstalled. Background notifications are disabled.");

    Ok(())
}

// ── service control ─────────────────────────────────────────────────────────

fn restart_service() {
    #[cfg(target_os = "macos")]
    {
        let label = LAUNCHD_LABEL;
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
        println!(
            "Restart not supported on this platform.\n\
             On Windows, restart the daemon by stopping and starting the task/service."
        );
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "function contains platform-specific blocks that cannot be further refactored without complicating the structure"
)]
fn reinstall_service(config: &mut Config) -> anyhow::Result<()> {
    println!("Reinstalling daemon service...");

    #[cfg(target_os = "macos")]
    {
        let plist_path = launchd_plist_path()?;
        let _ = Command::new("launchctl")
            .args(["unload", "-w", &plist_path.to_string_lossy()])
            .status();
        if plist_path.exists() {
            std::fs::remove_file(&plist_path)?;
            println!("  Removed  {}", plist_path.display());
        }
        let binary = current_binary_path()?;
        let home = home_dir()?;
        let plist = generate_launchd_plist(&binary, &home);
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
            config.setup.daemon_service_installed = true;
            config.save()?;
        }
        println!();
        println!("Daemon service reinstalled.");
    };

    #[cfg(target_os = "linux")]
    {
        let unit_path = systemd_unit_path()?;

        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", "scribe-daemon"])
            .status();

        if unit_path.exists() {
            std::fs::remove_file(&unit_path)?;
            println!("  Removed  {}", unit_path.display());
        }

        let binary = current_binary_path()?;
        let unit = generate_systemd_unit(&binary);

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

        config.setup.daemon_service_installed = true;
        config.save()?;

        println!();
        println!("Daemon service reinstalled.");
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!(
            "Daemon reinstall not supported on this platform.\n\
             On Windows, use Task Scheduler or NSSM to reinstall the service."
        );
    };

    Ok(())
}

// ── daemon loop ─────────────────────────────────────────────────────────────

#[cfg_attr(
    not(feature = "sync"),
    expect(unused_variables, reason = "only used by sync feature")
)]
fn run_loop(conn: &Arc<Mutex<Connection>>, interval_secs: Option<u64>, config: &Config) -> ! {
    let reminder_interval = Duration::from_secs(interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS));

    tracing::info!(interval_secs = reminder_interval.as_secs(), "daemon.start",);
    eprintln!(
        "Scribe daemon running — polling every {}s. Press Ctrl-C to stop.",
        reminder_interval.as_secs()
    );

    #[cfg(feature = "sync")]
    run_bootstrap();

    #[cfg(feature = "sync")]
    if let Err(e) = spawn_rest_server_thread(conn, config) {
        tracing::warn!(error = %e, "daemon: failed to start REST server");
    };

    let ops = ReminderOps::new(Arc::clone(conn));

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

#[cfg(feature = "sync")]
fn run_bootstrap() {
    crate::sync::keychain::KeychainStore::apply_bootstrap();
}

#[cfg(feature = "sync")]
fn run_sync_cycle(conn: &Arc<Mutex<Connection>>, config: &Config) {
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
        Ok((merged, summary)) => {
            if let Err(e) = merged.write_to_db(conn) {
                tracing::warn!(error = %e, "sync.write.error");
                return;
            }
            if let Err(e) = crate::db::save_sync_summary(conn, &summary) {
                tracing::warn!(error = %e, "sync.summary.save.error");
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

#[cfg(feature = "sync")]
fn spawn_rest_server_thread(conn: &Arc<Mutex<Connection>>, config: &Config) -> anyhow::Result<()> {
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

// ── shared helpers ─────────────────────────────────────────────────────────

/// Returns the path to the currently running `scribe` binary.
///
/// # Errors
///
/// Returns an error if the binary path cannot be determined.
fn current_binary_path() -> anyhow::Result<PathBuf> {
    std::env::current_exe().map_err(|e| anyhow::anyhow!("could not determine binary path: {e}"))
}

/// Returns the user's home directory.
///
/// # Errors
///
/// Returns an error when the home directory cannot be determined.
pub(crate) fn home_dir() -> anyhow::Result<PathBuf> {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_owned())
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))
}
