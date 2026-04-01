// Rust guideline compliant 2026-02-21
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
//! scribe service install    # install and start
//! scribe service uninstall  # stop and remove
//! scribe service status     # print current status
//! ```

use std::path::PathBuf;
use std::process::Command;

use clap::Subcommand;

use crate::config::Config;

// ── clap types ─────────────────────────────────────────────────────────────

/// Subcommands for `scribe service`.
#[derive(Debug, Subcommand)]
pub enum ServiceCommand {
    /// Install and start the background reminder daemon service.
    Install,
    /// Stop and remove the background reminder daemon service.
    Uninstall,
    /// Show whether the background daemon service is currently installed.
    Status,
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
pub fn run(cmd: &ServiceCommand, config: &mut Config) -> anyhow::Result<()> {
    match cmd {
        ServiceCommand::Install => install(config),
        ServiceCommand::Uninstall => uninstall(config),
        ServiceCommand::Status => status(config),
    }
}

// ── install ────────────────────────────────────────────────────────────────

/// Installs the daemon service for the current platform.
fn install(config: &mut Config) -> anyhow::Result<()> {
    if config.setup.daemon_service_installed {
        println!("Daemon service is already installed. Run `scribe service status` for details.");
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    install_launchd(config)?;

    #[cfg(target_os = "linux")]
    install_systemd(config)?;

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!(
            "Automatic service installation is not supported on this platform.\n\
             Run `scribe daemon` manually to start the background notification daemon."
        );
    }

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
        println!("Automatic service removal is not supported on this platform.");
    }

    Ok(())
}

// ── status ─────────────────────────────────────────────────────────────────

/// Prints the current service installation status.
fn status(config: &Config) -> anyhow::Result<()> {
    if config.setup.daemon_service_installed {
        #[cfg(target_os = "macos")]
        {
            let plist = launchd_plist_path()?;
            println!("Daemon service: installed");
            println!("  Plist:        {}", plist.display());
            println!("  Label:        {LAUNCHD_LABEL}");
        }
        #[cfg(target_os = "linux")]
        {
            let unit = systemd_unit_path()?;
            println!("Daemon service: installed");
            println!("  Unit file:    {}", unit.display());
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        println!("Daemon service: installed (manual)");
    } else {
        println!("Daemon service: not installed");
        println!("  Run `scribe service install` to set up automatic notifications.");
    }
    Ok(())
}

// ── macOS / launchd ────────────────────────────────────────────────────────

// DOCUMENTED-MAGIC: The label matches the plist filename by convention so
// that launchctl load/unload and status lookups are consistent.
#[cfg(target_os = "macos")]
const LAUNCHD_LABEL: &str = "com.scribe.daemon";

/// Returns `~/Library/LaunchAgents/com.scribe.daemon.plist`.
#[cfg(target_os = "macos")]
fn launchd_plist_path() -> anyhow::Result<PathBuf> {
    let home = home_dir()?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn install_launchd(config: &mut Config) -> anyhow::Result<()> {
    let binary = current_binary_path()?;
    let plist_path = launchd_plist_path()?;

    // Ensure LaunchAgents directory exists.
    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

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
        home = home_dir()?.display(),
    );

    std::fs::write(&plist_path, plist)?;
    println!("  Created  {}", plist_path.display());

    // Load (and immediately start) the agent.
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
fn systemd_unit_path() -> anyhow::Result<PathBuf> {
    let home = home_dir()?;
    Ok(home
        .join(".config")
        .join("systemd")
        .join("user")
        .join("scribe-daemon.service"))
}

#[cfg(target_os = "linux")]
fn install_systemd(config: &mut Config) -> anyhow::Result<()> {
    let binary = current_binary_path()?;
    let unit_path = systemd_unit_path()?;

    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

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

    std::fs::write(&unit_path, unit)?;
    println!("  Created  {}", unit_path.display());

    // Reload systemd user daemon so it picks up the new unit.
    let reload = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()?;

    if !reload.success() {
        anyhow::bail!("systemctl --user daemon-reload failed");
    }

    // Enable and start the unit.
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
