//! OS keychain abstraction for sync secrets.
//!
//! This module provides [`KeychainStore`], a thin wrapper around the [`keyring`]
//! crate that reads and writes sync secrets from the platform keychain (macOS
//! Keychain Services, Linux libsecret / gnome-keyring / kwallet, Windows
//! Credential Manager).
//!
//! ## Service name format
//!
//! Every keychain entry is identified by a **service name** of the form:
//!
//! ```text
//! scribe.sync.<provider>.<field>
//! ```
//!
//! For example, `scribe.sync.gist.token` stores the GitHub personal-access token
//! for the Gist provider.  This namespacing avoids collisions with other
//! applications that might store entries under a bare field name such as `"token"`.
//!
//! ## Username convention
//!
//! The [`keyring`] API requires both a *service name* and a *username*.  Scribe
//! always uses the fixed username `"scribe"` (see [`KEYCHAIN_USERNAME`]).  This is
//! not a real OS username — it is a stable namespace identifier that lets multiple
//! Scribe installations on the same machine share a single well-known set of
//! entries without having to know the OS account name at runtime.
//!
//! ## Headless Linux
//!
//! On headless Linux systems (servers, CI) the keychain daemon (gnome-keyring or
//! kwallet) is typically absent.  In that case [`Entry::new`] or
//! [`entry.get_password()`] will return a [`keyring::Error`] that surfaces here as
//! [`SyncError::Keychain`] with a message guiding the user to install a daemon or
//! use an alternative secret-storage strategy.

use keyring::Entry;

use crate::sync::SyncError;

/// Fixed username used for all Scribe keychain entries.
///
/// The `keyring` crate requires a username alongside a service name.  Scribe uses
/// the literal string `"scribe"` as a stable, well-known namespace identifier —
/// it is **not** the OS account name.  Changing this value would orphan all
/// previously stored secrets.
pub const KEYCHAIN_USERNAME: &str = "scribe";

/// Reads and writes sync secrets in the OS keychain.
///
/// All methods are associated functions (no receiver) because `KeychainStore`
/// holds no mutable state — every call constructs a transient [`Entry`] handle
/// and performs the operation immediately.
#[derive(Debug)]
pub struct KeychainStore;

impl KeychainStore {
    /// Returns the canonical service name for a provider secret.
    ///
    /// The format is `scribe.sync.<provider>.<field>`, e.g.
    /// `scribe.sync.gist.token`.  Both `provider` and `field` are embedded
    /// verbatim — callers must ensure they contain only URL-safe characters so
    /// that the resulting name is unambiguous across platforms.
    ///
    /// Accepts any string type for `provider` and `field`.
    #[must_use]
    pub fn service_name(provider: impl AsRef<str>, field: impl AsRef<str>) -> String {
        let provider = provider.as_ref();
        let field = field.as_ref();
        format!("scribe.sync.{provider}.{field}")
    }

    /// Retrieves a secret from the OS keychain.
    ///
    /// The service name is derived from `provider` and `field` via
    /// [`KeychainStore::service_name`]; the username is always
    /// [`KEYCHAIN_USERNAME`].  Accepts any string type for `provider` and
    /// `field`.
    ///
    /// On macOS, if the keychain is inaccessible (launchd session), this will
    /// check the bootstrap file first to handle the "Two Vaults" problem.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Keychain`] when:
    /// - The keychain entry cannot be created (e.g. no keychain daemon on
    ///   headless Linux).  The message instructs the user to install
    ///   gnome-keyring or kwallet.
    /// - The secret is not present in the keychain.  The message instructs the
    ///   user to run `scribe sync configure`.
    pub fn get(provider: impl AsRef<str>, field: impl AsRef<str>) -> Result<String, SyncError> {
        let service = Self::service_name(&provider, &field);

        if let Some(secret) = Self::get_from_bootstrap(provider.as_ref(), field.as_ref()) {
            tracing::debug!(service, "keychain.get: using bootstrap file");
            return Ok(secret);
        }

        // If SCRIBE_TEST_KEYCHAIN_BOOTSTRAP is set, we never fall back to real keychain.
        if std::env::var("SCRIBE_TEST_KEYCHAIN_BOOTSTRAP").is_ok() {
            return Err(SyncError::Keychain(format!(
                "secret '{service}' not found in mock keychain (bootstrap file). \
                 Ensure you called scribe::testing::keychain::set_secret() in your test setup."
            )));
        }

        let entry = Entry::new(&service, KEYCHAIN_USERNAME).map_err(|e| {
            SyncError::Keychain(format!(
                "sync requires a keychain daemon to store secrets securely. \
                 Install and start gnome-keyring or kwallet, then re-run \
                 `scribe sync configure`. (detail: {e})"
            ))
        })?;
        entry.get_password().map_err(|e| {
            SyncError::Keychain(format!(
                "secret '{service}' not found in keychain — run \
                 `scribe sync configure` to set it up. (detail: {e})"
            ))
        })
    }

    /// Retrieves a secret from the OS keychain, optionally failing silently
    /// instead of returning an error if the secret is missing.
    ///
    /// This is useful for daemons that may be configured to perform a
    /// bootstrap handoff if the secret is missing.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Keychain`] if the keychain entry cannot be created
    /// or if a keychain error occurs after the bootstrap handoff.
    pub fn get_optional(
        provider: impl AsRef<str>,
        field: impl AsRef<str>,
    ) -> Result<Option<String>, SyncError> {
        let service = Self::service_name(&provider, &field);

        if let Some(secret) = Self::get_from_bootstrap(provider.as_ref(), field.as_ref()) {
            tracing::debug!(service, "keychain.get_optional: using bootstrap file");
            return Ok(Some(secret));
        }

        // If we are in test mode (via environment variable), NEVER fall back to
        // the real OS keychain.
        if std::env::var("SCRIBE_TEST_KEYCHAIN_BOOTSTRAP").is_ok() {
            return Ok(None);
        }

        let entry = Entry::new(&service, KEYCHAIN_USERNAME).map_err(|e| {
            SyncError::Keychain(format!(
                "sync requires a keychain daemon to store secrets securely. \
                 Install and start gnome-keyring or kwallet, then re-run \
                 `scribe sync configure`. (detail: {e})"
            ))
        })?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => {
                Self::apply_bootstrap();
                match entry.get_password() {
                    Ok(p) => Ok(Some(p)),
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(e) => Err(SyncError::Keychain(format!(
                        "could not read from keychain after bootstrap: {e}"
                    ))),
                }
            }
            Err(e) => Err(SyncError::Keychain(format!(
                "could not read from keychain: {e}"
            ))),
        }
    }

    /// Retrieves a secret from the bootstrap file, if present.
    fn get_from_bootstrap(provider: &str, field: &str) -> Option<String> {
        let path = Self::bootstrap_path()?;
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&path).ok()?;
        let secrets: std::collections::HashMap<String, String> =
            serde_json::from_str(&content).ok()?;
        let key = format!("{provider}.{field}");
        secrets.get(&key).cloned()
    }

    /// Stores a secret in the OS keychain.
    ///
    /// The service name is derived from `provider` and `field` via
    /// [`KeychainStore::service_name`]; the username is always
    /// [`KEYCHAIN_USERNAME`].  Any existing value for the same service name is
    /// overwritten.  Accepts any string type for `provider`, `field`, and
    /// `secret`.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Keychain`] when the entry cannot be created (no
    /// keychain daemon) or when the write fails.
    pub fn set(
        provider: impl AsRef<str>,
        field: impl AsRef<str>,
        secret: impl AsRef<str>,
    ) -> Result<(), SyncError> {
        let provider_ref = provider.as_ref();
        let field_ref = field.as_ref();
        let secret_ref = secret.as_ref();

        // If we are in test mode (via environment variable), only update the bootstrap file.
        if std::env::var("SCRIBE_TEST_KEYCHAIN_BOOTSTRAP").is_ok() {
            Self::update_bootstrap(provider_ref, field_ref, secret_ref);
            return Ok(());
        }

        let service = Self::service_name(provider_ref, field_ref);
        let entry = Entry::new(&service, KEYCHAIN_USERNAME).map_err(|e| {
            SyncError::Keychain(format!(
                "sync requires a keychain daemon to store secrets securely. \
                 Install and start gnome-keyring or kwallet, then re-run \
                 `scribe sync configure`. (detail: {e})"
            ))
        })?;
        entry
            .set_password(secret_ref)
            .map_err(|e| SyncError::Keychain(format!("could not write to keychain: {e}")))?;

        Self::update_bootstrap(provider_ref, field_ref, secret_ref);
        Ok(())
    }

    /// Returns the path to the keychain bootstrap JSON file.
    #[must_use]
    pub fn bootstrap_path() -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var("SCRIBE_TEST_KEYCHAIN_BOOTSTRAP") {
            return Some(std::path::PathBuf::from(p));
        }
        directories::ProjectDirs::from("", "", "scribe")
            .map(|d| d.data_local_dir().join("keychain-bootstrap.json"))
    }

    /// Updates the bootstrap file so that background daemons can pick up secrets
    /// even if they run in a different session context (e.g., launchd vs terminal).
    fn update_bootstrap(provider: &str, field: &str, secret: &str) {
        let Some(path) = Self::bootstrap_path() else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        if let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(existing) = serde_json::from_str(&content)
        {
            map = existing;
        }

        map.insert(format!("{provider}.{field}"), secret.to_owned());

        if let Ok(json) = serde_json::to_string(&map) {
            let temp_path = path.with_extension("tmp");
            if std::fs::write(&temp_path, json).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600));
                }
                let _ = std::fs::rename(temp_path, path);
            }
        }
    }

    /// Checks for and applies a keychain bootstrap file.
    ///
    /// This bridges the "Two Vaults" problem on macOS where the CLI and the daemon
    /// (or multiple CLI invocations in different contexts) have different
    /// path identities, preventing access to secrets.
    pub fn apply_bootstrap() {
        // If we are in test mode (via environment variable), NEVER apply bootstrap to
        // the real OS keychain.
        if std::env::var("SCRIBE_TEST_KEYCHAIN_BOOTSTRAP").is_ok() {
            return;
        }

        let Some(path) = Self::bootstrap_path() else {
            return;
        };
        if !path.exists() {
            return;
        }

        if let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(secrets) =
                serde_json::from_str::<std::collections::HashMap<String, String>>(&content)
        {
            for (key, value) in secrets {
                let parts: Vec<&str> = key.split('.').collect();
                if parts.len() == 2 {
                    let service = Self::service_name(parts[0], parts[1]);
                    let entry = Entry::new(&service, KEYCHAIN_USERNAME);
                    match entry {
                        Ok(e) => match e.set_password(&value) {
                            Ok(()) => {
                                tracing::info!(service, "keychain.bootstrap.applied");
                            }
                            Err(e) => {
                                tracing::error!("set_password error: {:?}", e);
                            }
                        },
                        Err(e) => {
                            tracing::error!("Entry::new error: {:?}", e);
                        }
                    }
                }
            }
        }
        // Delete after consuming
        let _ = std::fs::remove_file(&path);
    }

    /// Removes a secret from the OS keychain.
    ///
    /// The service name is derived from `provider` and `field` via
    /// [`KeychainStore::service_name`]; the username is always
    /// [`KEYCHAIN_USERNAME`].  If the entry does not exist, this method
    /// succeeds silently.  Accepts any string type for `provider` and `field`.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Keychain`] when the entry cannot be created (no
    /// keychain daemon) or when the deletion fails for a reason other than the
    /// entry being absent.
    pub fn remove(provider: impl AsRef<str>, field: impl AsRef<str>) -> Result<(), SyncError> {
        let provider_ref = provider.as_ref();
        let field_ref = field.as_ref();

        // If we are in test mode (via environment variable), only clear from
        // the bootstrap file.
        if std::env::var("SCRIBE_TEST_KEYCHAIN_BOOTSTRAP").is_ok() {
            if let Some(p) = Self::bootstrap_path()
                && p.exists()
                && let Ok(content) = std::fs::read_to_string(&p)
                && let Ok(mut secrets) =
                    serde_json::from_str::<std::collections::HashMap<String, String>>(&content)
            {
                secrets.remove(&format!("{provider_ref}.{field_ref}"));
                if let Ok(json) = serde_json::to_string(&secrets) {
                    let _ = std::fs::write(&p, json);
                }
            }
            return Ok(());
        }

        let service = Self::service_name(provider_ref, field_ref);
        let entry = Entry::new(&service, KEYCHAIN_USERNAME).map_err(|e| {
            SyncError::Keychain(format!(
                "sync requires a keychain daemon to store secrets securely. \
                 Install and start gnome-keyring or kwallet, then re-run \
                 `scribe sync configure`. (detail: {e})"
            ))
        })?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SyncError::Keychain(format!(
                "could not remove keychain entry '{service}': {e}"
            ))),
        }
    }
}
