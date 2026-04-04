//! Mock OS keychain helpers for tests.
//!
//! Provides a way to initialize the `keyring` mock backend so that integration
//! tests do not prompt the user for their real OS keychain.
//!
//! # Parallel test safety
//!
//! Both `use_mock_keychain()` and `set_secret()` are safe to call from multiple
//! threads simultaneously. A `OnceLock` ensures the bootstrap path is initialized
//! exactly once before any secret operations.

use std::sync::{Mutex, OnceLock};

/// The bootstrap path, initialized once and shared across all threads.
static BOOTSTRAP_PATH: OnceLock<String> = OnceLock::new();

/// Global lock to prevent race conditions when multiple tests write to the
/// mock keychain file simultaneously.
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// Returns the bootstrap path, initializing it if necessary.
///
/// This is safe to call from multiple threads concurrently.
fn get_or_init_bootstrap_path() -> &'static str {
    BOOTSTRAP_PATH
        .get_or_init(|| {
            // Create a temporary file for the mock keychain.
            // Use the system temp directory with a unique name to avoid conflicts.
            let temp_dir = std::env::temp_dir();
            let path_obj =
                temp_dir.join(format!("scribe-mock-keychain-{}.json", std::process::id()));
            let path_str = path_obj.to_string_lossy().to_string();

            // Set the environment variable that KeychainStore::bootstrap_path() checks.
            // SAFETY: This is only called once via OnceLock initialization, so there
            // is no data race. The env var is specific to tests and doesn't affect
            // production behavior.
            unsafe { std::env::set_var("SCRIBE_TEST_KEYCHAIN_BOOTSTRAP", &path_str) };

            path_str
        })
        .as_str()
}

/// Initializes the mock keychain for tests.
///
/// This redirects all keychain operations to a temporary JSON file (the
/// "bootstrap" file) instead of the real OS keychain, preventing any
/// interactive prompts.
///
/// # Panics
///
/// Panics if the temporary directory cannot be created.
pub fn use_mock_keychain() {
    let _ = get_or_init_bootstrap_path();
}

/// Pre-loads the mock keychain with a secret for a specific provider and field.
///
/// This writes directly to the temporary JSON bootstrap file used for testing.
/// Safe to call without prior `use_mock_keychain()`.
///
/// # Errors
///
/// Returns an error if the bootstrap file cannot be written.
pub fn set_secret(provider: &str, field: &str, secret: &str) -> anyhow::Result<()> {
    let _lock = FILE_LOCK
        .lock()
        .map_err(|e| anyhow::anyhow!("failed to acquire mock keychain lock: {e}"))?;

    let path_str = get_or_init_bootstrap_path();
    let path = std::path::PathBuf::from(path_str);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Create the file if it doesn't exist.
    if !path.exists() {
        std::fs::write(&path, "{}")?;
    }

    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let content = std::fs::read_to_string(&path)?;
    if let Ok(existing) = serde_json::from_str(&content) {
        map = existing;
    }

    map.insert(format!("{provider}.{field}"), secret.to_owned());
    let json = serde_json::to_string(&map)?;
    std::fs::write(&path, json)?;

    Ok(())
}
