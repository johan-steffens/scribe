// Mock configuration helpers for tests.
//
// Provides [`TestConfig`] for creating isolated, temporary [`crate::config::Config`]
// instances without reading or writing to the user's real config directory.

use std::path::PathBuf;

/// A test configuration with optional overrides for the database path.
///
/// This struct simplifies creating a [`crate::config::Config`] in tests where
/// you need a known configuration state without side effects on the real
/// `$XDG_CONFIG_HOME/scribe/` directory.
///
/// # Example
///
/// ```
/// use crate::testing::config::TestConfig;
///
/// // Fully default config
/// let config = TestConfig::new();
///
/// // Override the database path
/// let config = TestConfig::with_db_path("/tmp/my-test.db");
/// ```
#[derive(Debug)]
pub struct TestConfig {
    /// Inner config that will be returned via `as_config()`.
    config: crate::config::Config,
}

impl TestConfig {
    /// Creates a new test config with all defaults and no database override.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: crate::config::Config::default(),
        }
    }

    /// Creates a new test config that points at a specific database file.
    ///
    /// This is useful when pairing with [`crate::testing::db::TestDb::tempfile()`]
    /// to ensure the config's `db_path()` returns the same path as the
    /// temporary database.
    #[must_use]
    pub fn with_db_path(db_path: impl Into<PathBuf>) -> Self {
        let db_path = db_path.into();
        let mut config = crate::config::Config::default();
        config.db_path = Some(db_path);
        Self { config }
    }

    /// Creates a new test config for a temporary file database.
    ///
    /// This is a convenience method that combines `TestDb::new_in_dir()` with
    /// `TestConfig::with_db_path()` so the config points to the same temporary
    /// file that the database is using.
    ///
    /// # Example
    ///
    /// ```
    /// let (config, test_db) = crate::testing::config::TestConfig::with_temp_db();
    /// // Use config with the test...
    /// ```
    pub fn with_temp_db() -> (Self, super::db::TestDb) {
        let dir = tempfile::tempdir().expect("tempdir should succeed");
        let db_path = dir.path().join("test.db");
        let test_db = super::db::TestDb::new_in_dir(dir);
        let config = Self::with_db_path(&db_path);
        (config, test_db)
    }

    /// Returns a reference to the inner [`Config`].
    #[must_use]
    pub fn as_config(&self) -> &crate::config::Config {
        &self.config
    }

    /// Returns the configured database path, or the XDG default if none was set.
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.config.db_path()
    }

    /// Sets the `setup` fields to simulate a fully-configured installation.
    pub fn with_setup_completed(mut self) -> Self {
        self.config.setup.daemon_service_installed = true;
        self.config.setup.agent_installed = true;
        self
    }

    /// Sets `notifications_enabled` for the test.
    pub fn with_notifications(mut self, enabled: bool) -> Self {
        self.config.notifications_enabled = enabled;
        self
    }

    /// Sets the date format string.
    pub fn with_date_format(mut self, format: impl Into<String>) -> Self {
        self.config.date_format = format.into();
        self
    }

    /// Sets the time format string.
    pub fn with_time_format(mut self, format: impl Into<String>) -> Self {
        self.config.time_format = format.into();
        self
    }
}

impl Default for TestConfig {
    fn default() -> Self {
        Self::new()
    }
}
