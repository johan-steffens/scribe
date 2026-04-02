// Rust guideline compliant 2026-02-21
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
    #[must_use]
    pub fn service_name(provider: &str, field: &str) -> String {
        format!("scribe.sync.{provider}.{field}")
    }

    /// Retrieves a secret from the OS keychain.
    ///
    /// The service name is derived from `provider` and `field` via
    /// [`KeychainStore::service_name`]; the username is always
    /// [`KEYCHAIN_USERNAME`].
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Keychain`] when:
    /// - The keychain entry cannot be created (e.g. no keychain daemon on
    ///   headless Linux).  The message instructs the user to install
    ///   gnome-keyring or kwallet.
    /// - The secret is not present in the keychain.  The message instructs the
    ///   user to run `scribe sync configure`.
    pub fn get(provider: &str, field: &str) -> Result<String, SyncError> {
        let service = Self::service_name(provider, field);
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

    /// Stores a secret in the OS keychain.
    ///
    /// The service name is derived from `provider` and `field` via
    /// [`KeychainStore::service_name`]; the username is always
    /// [`KEYCHAIN_USERNAME`].  Any existing value for the same service name is
    /// overwritten.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Keychain`] when the entry cannot be created (no
    /// keychain daemon) or when the write fails.
    pub fn set(provider: &str, field: &str, secret: &str) -> Result<(), SyncError> {
        let service = Self::service_name(provider, field);
        let entry = Entry::new(&service, KEYCHAIN_USERNAME).map_err(|e| {
            SyncError::Keychain(format!(
                "sync requires a keychain daemon to store secrets securely. \
                 Install and start gnome-keyring or kwallet, then re-run \
                 `scribe sync configure`. (detail: {e})"
            ))
        })?;
        entry
            .set_password(secret)
            .map_err(|e| SyncError::Keychain(format!("could not write to keychain: {e}")))
    }

    /// Removes a secret from the OS keychain.
    ///
    /// The service name is derived from `provider` and `field` via
    /// [`KeychainStore::service_name`]; the username is always
    /// [`KEYCHAIN_USERNAME`].  If the entry does not exist, this method
    /// succeeds silently.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Keychain`] when the entry cannot be created (no
    /// keychain daemon) or when the deletion fails for a reason other than the
    /// entry being absent.
    pub fn remove(provider: &str, field: &str) -> Result<(), SyncError> {
        let service = Self::service_name(provider, field);
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
