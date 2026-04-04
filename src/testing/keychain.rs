// Mock OS keychain helpers for tests.
//
// Providers a way to initialize the `keyring` mock backend so that integration
// tests do not prompt the user for their real OS keychain.

use std::sync::Once;

static INIT: Once = Once::new();

/// Initializes the mock keychain for tests.
///
/// This redirects all keychain operations to a temporary JSON file (the
/// "bootstrap" file) instead of the real OS keychain, preventing any
/// interactive prompts.
pub fn use_mock_keychain() {
    INIT.call_once(|| {
        // Create a temporary file for the mock keychain.
        let dir = tempfile::tempdir().expect("failed to create temp dir for mock keychain");
        let path = dir.path().join("mock-keychain.json");

        // Set the environment variable that KeychainStore::bootstrap_path() checks.
        // SAFETY: This is called within call_once during test initialization.
        unsafe {
            std::env::set_var(
                "SCRIBE_TEST_KEYCHAIN_BOOTSTRAP",
                path.to_string_lossy().as_ref(),
            );
        }

        // Leak the directory so it persists for the duration of the test process.
        // The OS will clean it up on exit.
        std::mem::forget(dir);
    });
}

/// Pre-loads the mock keychain with a secret for a specific provider and field.
///
/// This writes directly to the temporary JSON bootstrap file used for testing.
///
/// # Errors
///
/// Returns an error if the bootstrap file cannot be written.
pub fn set_secret(provider: &str, field: &str, secret: &str) -> anyhow::Result<()> {
    use crate::sync::keychain::KeychainStore;

    let path = KeychainStore::bootstrap_path()
        .ok_or_else(|| anyhow::anyhow!("mock keychain path not available"))?;

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        if let Ok(existing) = serde_json::from_str(&content) {
            map = existing;
        }
    }

    map.insert(format!("{provider}.{field}"), secret.to_owned());
    let json = serde_json::to_string(&map)?;
    std::fs::write(&path, json)?;

    Ok(())
}
