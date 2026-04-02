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

// ── sync config ────────────────────────────────────────────────────────────

#[cfg(feature = "sync")]
/// Active sync provider name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncProvider {
    /// GitHub Gist-backed sync (default).
    #[default]
    Gist,
    /// Amazon S3-compatible object storage.
    S3,
    /// Apple iCloud Drive file sync.
    ICloud,
    /// JSONBin.io cloud storage.
    JsonBin,
    /// Dropbox file sync.
    Dropbox,
    /// Custom REST server sync.
    Rest,
    /// Local or network file path sync.
    File,
}

#[cfg(feature = "sync")]
/// Role of this machine in the REST sync model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RestRole {
    /// Receives state from a master node (default).
    #[default]
    Client,
    /// Acts as the authoritative source of truth.
    Master,
}

#[cfg(feature = "sync")]
/// Non-secret configuration for the GitHub Gist sync provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncGistConfig {
    /// ID of the Gist used to store the sync state document.
    pub gist_id: String,
}

/// Default S3 object key for the sync state document.
#[cfg(feature = "sync")]
const DEFAULT_S3_KEY: &str = "scribe-state.json";

/// Default AWS region for S3 sync.
#[cfg(feature = "sync")]
const DEFAULT_S3_REGION: &str = "us-east-1";

#[cfg(feature = "sync")]
fn default_s3_key() -> String {
    DEFAULT_S3_KEY.to_owned()
}

#[cfg(feature = "sync")]
fn default_s3_region() -> String {
    DEFAULT_S3_REGION.to_owned()
}

#[cfg(feature = "sync")]
/// Non-secret configuration for the S3-compatible sync provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncS3Config {
    /// S3-compatible endpoint URL (e.g. `https://s3.amazonaws.com`).
    pub endpoint: String,
    /// Bucket name to store the sync state document.
    pub bucket: String,
    /// Object key for the sync state document.
    #[serde(default = "default_s3_key")]
    pub key: String,
    /// AWS region name.
    #[serde(default = "default_s3_region")]
    pub region: String,
}

#[cfg(feature = "sync")]
impl Default for SyncS3Config {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            bucket: String::new(),
            key: DEFAULT_S3_KEY.to_owned(),
            region: DEFAULT_S3_REGION.to_owned(),
        }
    }
}

/// Default iCloud Drive path for the sync state document.
#[cfg(feature = "sync")]
const DEFAULT_ICLOUD_PATH: &str =
    "~/Library/Mobile Documents/com~apple~CloudDocs/scribe-state.json";

#[cfg(feature = "sync")]
fn default_icloud_path() -> String {
    DEFAULT_ICLOUD_PATH.to_owned()
}

#[cfg(feature = "sync")]
/// Non-secret configuration for the iCloud Drive sync provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncICloudConfig {
    /// File path inside iCloud Drive for the sync state document.
    #[serde(default = "default_icloud_path")]
    pub path: String,
}

#[cfg(feature = "sync")]
impl Default for SyncICloudConfig {
    fn default() -> Self {
        Self {
            path: DEFAULT_ICLOUD_PATH.to_owned(),
        }
    }
}

#[cfg(feature = "sync")]
/// Non-secret configuration for the JSONBin.io sync provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncJsonBinConfig {
    /// ID of the bin used to store the sync state document.
    pub bin_id: String,
}

/// Default Dropbox file path for the sync state document.
#[cfg(feature = "sync")]
const DEFAULT_DROPBOX_PATH: &str = "/scribe-state.json";

#[cfg(feature = "sync")]
fn default_dropbox_path() -> String {
    DEFAULT_DROPBOX_PATH.to_owned()
}

#[cfg(feature = "sync")]
/// Non-secret configuration for the Dropbox sync provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDropboxConfig {
    /// Dropbox path for the sync state document.
    #[serde(default = "default_dropbox_path")]
    pub path: String,
}

#[cfg(feature = "sync")]
impl Default for SyncDropboxConfig {
    fn default() -> Self {
        Self {
            path: DEFAULT_DROPBOX_PATH.to_owned(),
        }
    }
}

/// Default TCP port for the REST sync server.
#[cfg(feature = "sync")]
const DEFAULT_REST_PORT: u16 = 7171;

#[cfg(feature = "sync")]
fn default_rest_port() -> u16 {
    DEFAULT_REST_PORT
}

#[cfg(feature = "sync")]
/// Non-secret configuration for the custom REST sync provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRestConfig {
    /// Base URL of the REST sync server (e.g. `http://192.168.1.1:7171`).
    pub url: String,
    /// Role of this machine in the REST sync model.
    pub role: RestRole,
    /// TCP port the REST server listens on.
    #[serde(default = "default_rest_port")]
    pub port: u16,
}

#[cfg(feature = "sync")]
impl Default for SyncRestConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            role: RestRole::default(),
            port: DEFAULT_REST_PORT,
        }
    }
}

#[cfg(feature = "sync")]
/// Non-secret configuration for the local/network file sync provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncFileConfig {
    /// Filesystem path to the sync state document.
    pub path: String,
}

/// Default sync polling interval in seconds.
#[cfg(feature = "sync")]
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 60;

#[cfg(feature = "sync")]
fn default_sync_interval_secs() -> u64 {
    DEFAULT_SYNC_INTERVAL_SECS
}

#[cfg(feature = "sync")]
/// Top-level `[sync]` configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Whether the sync subsystem is active.
    #[serde(default)]
    pub enabled: bool,
    /// Which sync provider to use.
    #[serde(default)]
    pub provider: SyncProvider,
    /// How often (in seconds) to poll for remote changes.
    #[serde(default = "default_sync_interval_secs")]
    pub interval_secs: u64,
    /// GitHub Gist provider configuration.
    #[serde(default)]
    pub gist: SyncGistConfig,
    /// S3-compatible provider configuration.
    #[serde(default)]
    pub s3: SyncS3Config,
    /// iCloud Drive provider configuration.
    #[serde(default)]
    pub icloud: SyncICloudConfig,
    /// JSONBin.io provider configuration.
    #[serde(default)]
    pub jsonbin: SyncJsonBinConfig,
    /// Dropbox provider configuration.
    #[serde(default)]
    pub dropbox: SyncDropboxConfig,
    /// Custom REST server provider configuration.
    #[serde(default)]
    pub rest: SyncRestConfig,
    /// Local/network file provider configuration.
    #[serde(default)]
    pub file: SyncFileConfig,
}

#[cfg(feature = "sync")]
impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: SyncProvider::default(),
            interval_secs: DEFAULT_SYNC_INTERVAL_SECS,
            gist: SyncGistConfig::default(),
            s3: SyncS3Config::default(),
            icloud: SyncICloudConfig::default(),
            jsonbin: SyncJsonBinConfig::default(),
            dropbox: SyncDropboxConfig::default(),
            rest: SyncRestConfig::default(),
            file: SyncFileConfig::default(),
        }
    }
}

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
    #[cfg(feature = "sync")]
    #[serde(default)]
    sync: SyncConfig,
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
    /// Cloud sync configuration (requires the `sync` feature).
    #[cfg(feature = "sync")]
    pub sync: SyncConfig,
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
            #[cfg(feature = "sync")]
            sync: raw.sync,
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
            #[cfg(feature = "sync")]
            sync: self.sync.clone(),
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
            #[cfg(feature = "sync")]
            sync: SyncConfig::default(),
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

    #[cfg(feature = "sync")]
    #[test]
    fn test_sync_config_defaults_to_disabled() {
        let cfg = Config::default();
        assert!(!cfg.sync.enabled);
        assert_eq!(cfg.sync.provider, SyncProvider::Gist);
        assert_eq!(cfg.sync.interval_secs, 60);
    }

    #[cfg(feature = "sync")]
    #[test]
    fn test_sync_config_round_trip() {
        let mut cfg = Config::default();
        cfg.sync.enabled = true;
        cfg.sync.provider = SyncProvider::S3;
        cfg.sync.s3.bucket = "my-bucket".to_owned();
        let raw = cfg.to_raw();
        let restored = Config::from_raw(raw);
        assert!(restored.sync.enabled);
        assert_eq!(restored.sync.provider, SyncProvider::S3);
        assert_eq!(restored.sync.s3.bucket, "my-bucket");
    }
}
