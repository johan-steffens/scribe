// Rust guideline compliant 2026-02-21
//! Unit tests for [`crate::config::Config`].

use std::path::PathBuf;

// Modules needed for sync config tests
#[cfg(feature = "sync")]
use scribe::config::SyncProvider;

use scribe::config::Config;
use scribe::testing::config::TestConfig;

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
    let test_cfg = TestConfig::with_db_path("/tmp/test.db");
    let cfg = test_cfg.as_config();
    assert_eq!(cfg.db_path(), PathBuf::from("/tmp/test.db"));
}

#[test]
fn test_db_path_returns_xdg_default_when_unset() {
    let cfg = Config::default();
    let db = cfg.db_path();
    assert_eq!(db.file_name().and_then(|n| n.to_str()), Some("scribe.db"));
}

#[cfg(feature = "sync")]
#[test]
fn test_sync_config_defaults_to_disabled() {
    let cfg = Config::default();
    assert!(!cfg.sync.enabled);
    assert_eq!(cfg.sync.provider, SyncProvider::Gist);
    assert_eq!(cfg.sync.interval_secs, 60);
}
