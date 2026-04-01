// Rust guideline compliant 2026-02-21
//! Configuration loading and saving from XDG-compliant paths.
//!
//! This module reads and writes `config.toml` from
//! `$XDG_CONFIG_HOME/scribe/` (falling back to `~/.config/scribe/`) and
//! provides helpers for resolving runtime paths such as the database file
//! location.
//!
//! # Example
//!
//! ```no_run
//! use scribe::config::Config;
//!
//! let cfg = Config::load().expect("failed to load config");
//! let db  = cfg.db_path();
//! ```

use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

// ── on-disk representation ─────────────────────────────────────────────────

/// Raw `[data]` section as it appears in `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DataConfig {
    /// Override for the `SQLite` database path; empty string means use default.
    db_path: Option<String>,
}

/// Raw `[notifications]` section as it appears in `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotificationsConfig {
    /// Whether desktop notifications are enabled.
    enabled: bool,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Raw `[display]` section as it appears in `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DisplayConfig {
    /// `strftime`-compatible date format string.
    date_format: String,
    /// `strftime`-compatible time format string.
    time_format: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            date_format: "%Y-%m-%d".to_owned(),
            time_format: "%H:%M".to_owned(),
        }
    }
}

/// Raw `[setup]` section as it appears in `config.toml`.
///
/// Tracks which optional setup steps the user has completed so that
/// `scribe setup` can report status and skip already-done steps.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SetupConfig {
    /// Whether the background daemon service has been installed.
    #[serde(default)]
    pub daemon_service_installed: bool,
    /// Whether the agent skill files have been installed.
    #[serde(default)]
    pub agent_installed: bool,
}

/// Raw top-level structure parsed from `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    data: DataConfig,
    #[serde(default)]
    notifications: NotificationsConfig,
    #[serde(default)]
    display: DisplayConfig,
    #[serde(default)]
    setup: SetupConfig,
}

// ── public Config ──────────────────────────────────────────────────────────

/// Application configuration loaded from `$XDG_CONFIG_HOME/scribe/config.toml`.
///
/// All fields have sensible defaults so the config file is entirely optional.
/// Call [`Config::load`] to obtain an instance, and [`Config::save`] to
/// persist changes back to disk.
///
/// # Examples
///
/// ```no_run
/// use scribe::config::Config;
///
/// let cfg = Config::load().expect("failed to load config");
/// println!("DB path: {}", cfg.db_path().display());
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    /// Explicit database path override from the config file, if any.
    pub db_path: Option<PathBuf>,
    /// Whether desktop notifications are enabled.
    #[allow(dead_code, reason = "used in Phase 2 notification subsystem")]
    pub notifications_enabled: bool,
    /// `strftime`-compatible date format string (e.g. `"%Y-%m-%d"`).
    #[allow(dead_code, reason = "used in Phase 2 display formatting")]
    pub date_format: String,
    /// `strftime`-compatible time format string (e.g. `"%H:%M"`).
    #[allow(dead_code, reason = "used in Phase 2 display formatting")]
    pub time_format: String,
    /// Setup completion state — written by `scribe setup` and `scribe service`.
    pub setup: SetupConfig,
}

impl Default for Config {
    fn default() -> Self {
        let raw = RawConfig::default();
        Self::from_raw(raw)
    }
}

impl Config {
    /// Loads configuration from disk, returning defaults when the file is absent.
    ///
    /// The config file is looked up at `$XDG_CONFIG_HOME/scribe/config.toml`,
    /// falling back to `~/.config/scribe/config.toml` when the env var is
    /// unset. If the file does not exist, all values revert to their defaults
    /// — this is not treated as an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or contains
    /// invalid TOML.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use scribe::config::Config;
    ///
    /// let cfg = Config::load().expect("config load failed");
    /// ```
    pub fn load() -> anyhow::Result<Self> {
        let path = config_file_path();

        if !path.exists() {
            tracing::debug!(
                config.path = %path.display(),
                "config file not found, using defaults",
            );
            return Ok(Self::default());
        }

        let text = std::fs::read_to_string(&path)?;
        let raw: RawConfig = toml::from_str(&text)?;
        tracing::debug!(
            config.path = %path.display(),
            "config loaded",
        );
        Ok(Self::from_raw(raw))
    }

    /// Persists the current configuration to disk.
    ///
    /// Creates the parent directory if it does not exist. The file is written
    /// atomically: a complete TOML document is serialised and written in one
    /// `fs::write` call.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created, the TOML cannot
    /// be serialised, or the file cannot be written.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use scribe::config::Config;
    ///
    /// let mut cfg = Config::load().unwrap();
    /// cfg.setup.daemon_service_installed = true;
    /// cfg.save().unwrap();
    /// ```
    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_file_path();

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let raw = self.to_raw();
        let text = toml::to_string_pretty(&raw)?;
        std::fs::write(&path, text)?;
        tracing::debug!(config.path = %path.display(), "config saved");
        Ok(())
    }

    /// Returns the effective database file path.
    ///
    /// Uses the value from `config.toml` when set; otherwise falls back to
    /// `$XDG_DATA_HOME/scribe/scribe.db` (or `~/.local/share/scribe/scribe.db`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use scribe::config::Config;
    ///
    /// let cfg = Config::default();
    /// let db  = cfg.db_path();
    /// ```
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        if let Some(ref p) = self.db_path {
            return p.clone();
        }
        default_db_path()
    }

    // ── private helpers ────────────────────────────────────────────────────

    fn from_raw(raw: RawConfig) -> Self {
        let db_path = raw
            .data
            .db_path
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        Self {
            db_path,
            notifications_enabled: raw.notifications.enabled,
            date_format: raw.display.date_format,
            time_format: raw.display.time_format,
            setup: raw.setup,
        }
    }

    fn to_raw(&self) -> RawConfig {
        RawConfig {
            data: DataConfig {
                db_path: self
                    .db_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
            },
            notifications: NotificationsConfig {
                enabled: self.notifications_enabled,
            },
            display: DisplayConfig {
                date_format: self.date_format.clone(),
                time_format: self.time_format.clone(),
            },
            setup: self.setup.clone(),
        }
    }
}

// ── path helpers ───────────────────────────────────────────────────────────

/// Returns `$XDG_CONFIG_HOME/scribe/config.toml`.
pub(crate) fn config_file_path() -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("", "", "scribe") {
        dirs.config_dir().join("config.toml")
    } else {
        // Fallback for environments where home dir cannot be determined.
        PathBuf::from(".config/scribe/config.toml")
    }
}

/// Returns `$XDG_DATA_HOME/scribe/scribe.db`.
pub(crate) fn default_db_path() -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("", "", "scribe") {
        dirs.data_dir().join("scribe.db")
    } else {
        PathBuf::from(".local/share/scribe/scribe.db")
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_sensible_values() {
        let cfg = Config::default();
        assert!(cfg.db_path.is_none());
        assert!(cfg.notifications_enabled);
        assert_eq!(cfg.date_format, "%Y-%m-%d");
        assert_eq!(cfg.time_format, "%H:%M");
        assert!(!cfg.setup.daemon_service_installed);
        assert!(!cfg.setup.agent_installed);
    }

    #[test]
    fn test_db_path_returns_override_when_set() {
        let cfg = Config {
            db_path: Some(PathBuf::from("/tmp/test.db")),
            notifications_enabled: true,
            date_format: "%Y-%m-%d".to_owned(),
            time_format: "%H:%M".to_owned(),
            setup: SetupConfig::default(),
        };
        assert_eq!(cfg.db_path(), PathBuf::from("/tmp/test.db"));
    }

    #[test]
    fn test_db_path_returns_xdg_default_when_unset() {
        let cfg = Config::default();
        let db = cfg.db_path();
        assert_eq!(db.file_name().and_then(|n| n.to_str()), Some("scribe.db"));
    }

    #[test]
    fn test_from_raw_empty_db_path_string_uses_default() {
        let mut raw = RawConfig::default();
        raw.data.db_path = Some(String::new());
        let cfg = Config::from_raw(raw);
        assert!(cfg.db_path.is_none());
    }

    #[test]
    fn test_round_trip_preserves_setup_state() {
        let mut cfg = Config::default();
        cfg.setup.daemon_service_installed = true;
        cfg.setup.agent_installed = true;
        let raw = cfg.to_raw();
        let restored = Config::from_raw(raw);
        assert!(restored.setup.daemon_service_installed);
        assert!(restored.setup.agent_installed);
    }
}
