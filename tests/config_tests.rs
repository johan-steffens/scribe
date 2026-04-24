//! Unit tests for [`crate::config::Config`].

use std::path::PathBuf;
use std::sync::Mutex;

// Modules needed for sync config tests
#[cfg(feature = "sync")]
use scribe::config::SyncProvider;

use scribe::config::Config;
use scribe::testing::config::TestConfig;

// Serializes access to the global EDITOR env var during tests.
static EDITOR_MTX: Mutex<()> = Mutex::new(());

/// Runs `f` with EDITOR set to `editor`, restoring the previous value afterwards.
fn with_editor_env(editor: &str, f: impl FnOnce()) {
    // Serialize test editor access to avoid races on the global EDITOR var.
    let _guard = EDITOR_MTX.lock().unwrap();
    let old_editor = std::env::var("EDITOR").ok();
    // SAFETY: Setting environment variables is inherently unsafe on some
    // platforms, but test editor values are confined to the test process.
    unsafe { std::env::set_var("EDITOR", editor) };
    f();
    // SAFETY: restoring or removing the EDITOR var is safe for the same reason.
    unsafe {
        if let Some(v) = old_editor {
            std::env::set_var("EDITOR", v);
        } else {
            std::env::remove_var("EDITOR");
        }
    }
}

#[test]
fn test_default_config_has_sensible_values() {
    let cfg = Config::default();
    assert!(cfg.db_path.is_none());
    assert!(cfg.notifications_enabled);
    assert_eq!(cfg.date_format, "%Y-%m-%d");
    assert_eq!(cfg.time_format, "%H:%M");
    assert!(cfg.note_editor.is_none());
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

#[test]
fn test_note_editor_from_config() {
    let test_cfg = TestConfig::new().with_note_editor("nvim");
    let cfg = test_cfg.as_config();
    assert_eq!(cfg.note_editor, Some("nvim".to_owned()));
}

#[test]
fn test_note_editor_method_prefers_config_over_env() {
    with_editor_env("emacs", || {
        let test_cfg = TestConfig::new().with_note_editor("nvim");
        let cfg = test_cfg.as_config();
        // Config value takes precedence over EDITOR env var
        assert_eq!(cfg.note_editor(), "nvim");
    });
}

#[test]
fn test_note_editor_method_falls_back_to_editor_env() {
    with_editor_env("emacs", || {
        let cfg = Config::default();
        // No config value, falls back to EDITOR env var
        assert_eq!(cfg.note_editor(), "emacs");
    });
}

#[test]
fn test_note_editor_method_falls_back_to_vim() {
    // Serialize test editor access to avoid races on the global EDITOR var.
    let _guard = EDITOR_MTX.lock().unwrap();
    // Ensure EDITOR is not set before running this test.
    // SAFETY: Removing EDITOR env var is safe for testing.
    unsafe { std::env::remove_var("EDITOR") };
    let cfg = Config::default();
    // No config value, EDITOR not set, defaults to vim
    assert_eq!(cfg.note_editor(), "vim");
}
