// Rust guideline compliant 2026-02-21
//! Sync provider implementations and configuration-driven factory.
//!
//! Each provider implements [`crate::sync::SyncProvider`]. The active provider
//! is constructed from [`crate::config::Config`] via [`from_config`].

pub mod dropbox;
pub mod file;
pub mod gist;
#[cfg(target_os = "macos")]
pub mod icloud;
pub mod jsonbin;
pub mod rest;
pub mod s3;

use crate::config::{Config, SyncProvider as SyncProviderName};
use crate::sync::{SyncError, SyncProvider};

// ── shared constants ───────────────────────────────────────────────────────

/// HTTP `User-Agent` header sent by all sync providers.
///
/// Identifies Scribe to remote APIs for rate-limiting and logging purposes.
/// Update the version suffix when the sync protocol changes in a
/// breaking way. Format: `<application>/<version>`.
pub(crate) const USER_AGENT: &str = "scribe-sync/1.0";

// ── provider factory ───────────────────────────────────────────────────────

/// Constructs the active sync provider from the current configuration.
///
/// Returns `None` when `sync.enabled = false`. Returns an error if the
/// provider cannot be constructed (e.g. missing required config fields).
///
/// # Errors
///
/// Returns [`SyncError`] if the active provider configuration is invalid.
pub fn from_config(config: &Config) -> Result<Option<Box<dyn SyncProvider>>, SyncError> {
    if !config.sync.enabled {
        return Ok(None);
    }
    let provider: Box<dyn SyncProvider> = match config.sync.provider {
        SyncProviderName::Gist => {
            let gist_id = if config.sync.gist.gist_id.is_empty() {
                None
            } else {
                Some(config.sync.gist.gist_id.clone())
            };
            Box::new(gist::GistProvider::new(gist_id)?)
        }
        SyncProviderName::S3 => Box::new(s3::S3Provider::new(
            config.sync.s3.endpoint.clone(),
            config.sync.s3.bucket.clone(),
            config.sync.s3.key.clone(),
            config.sync.s3.region.clone(),
        )?),
        SyncProviderName::JsonBin => {
            let bin_id = if config.sync.jsonbin.bin_id.is_empty() {
                None
            } else {
                Some(config.sync.jsonbin.bin_id.clone())
            };
            Box::new(jsonbin::JsonBinProvider::new(bin_id)?)
        }
        SyncProviderName::Dropbox => Box::new(dropbox::DropboxProvider::new(
            config.sync.dropbox.path.clone(),
        )?),
        SyncProviderName::Rest => Box::new(rest::RestProvider::new(config.sync.rest.url.clone())?),
        SyncProviderName::File => Box::new(file::FileProvider::new(config.sync.file.path.clone())),
        SyncProviderName::ICloud => {
            #[cfg(target_os = "macos")]
            {
                Box::new(icloud::ICloudProvider::new(&config.sync.icloud.path))
            }
            #[cfg(not(target_os = "macos"))]
            {
                return Err(SyncError::Other(
                    "iCloud Drive sync is only available on macOS".to_owned(),
                ));
            }
        }
    };
    Ok(Some(provider))
}
