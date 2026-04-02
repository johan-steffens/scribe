// Rust guideline compliant 2026-02-21
//! `scribe sync` — manual sync, configuration, and status subcommands.
//!
//! Provides three subcommands:
//! - `scribe sync` — one-shot manual sync
//! - `scribe sync configure` — interactive provider configuration
//! - `scribe sync status` — show last sync time and provider
//!
//! All subcommands require the `sync` Cargo feature.

use std::io::Write as _;

use chrono::Utc;
use clap::{Args, Subcommand};
use serde_json::json;
use uuid::Uuid;

use crate::cli::project::OutputFormat;
use crate::config::SyncProvider;
use crate::sync::engine::SyncState;
use crate::sync::keychain::KeychainStore;

// ── top-level sync command ─────────────────────────────────────────────────

/// Arguments for `scribe sync`.
#[derive(Debug, Args)]
pub struct SyncCommand {
    /// Sync subcommand; if omitted, runs a one-shot manual sync.
    #[command(subcommand)]
    pub subcommand: Option<SyncSubcommand>,
    /// Output format (applies to the one-shot sync and status output).
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Subcommands for `scribe sync`.
#[derive(Debug, Subcommand)]
pub enum SyncSubcommand {
    /// Configure the active sync provider and store secrets in the keychain.
    Configure(SyncConfigureArgs),
    /// Show sync status: provider, last sync time, last error.
    Status(SyncStatusArgs),
}

// ── subcommand structs ─────────────────────────────────────────────────────

/// Arguments for `scribe sync configure`.
#[derive(Debug, Args)]
pub struct SyncConfigureArgs {
    /// Provider to configure; omit to be prompted interactively.
    #[arg(long)]
    pub provider: Option<String>,
    /// Remove stored secrets for the active provider from the keychain.
    #[arg(long)]
    pub remove: bool,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe sync status`.
#[derive(Debug, Args)]
pub struct SyncStatusArgs {
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

// ── dispatch ───────────────────────────────────────────────────────────────

/// Executes `scribe sync [subcommand]`.
///
/// # Errors
///
/// Returns an error if the subcommand fails or the one-shot sync fails.
pub fn run(
    args: &SyncCommand,
    config: &mut crate::config::Config,
    conn: Option<&std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>>,
) -> anyhow::Result<()> {
    match &args.subcommand {
        Some(SyncSubcommand::Configure(cfg_args)) => run_configure(cfg_args, config),
        Some(SyncSubcommand::Status(status_args)) => run_status(status_args, config),
        None => {
            if !config.sync.enabled {
                anyhow::bail!(
                    "sync is not enabled — run `scribe sync configure` to set up a provider"
                );
            }
            let conn = conn.ok_or_else(|| {
                anyhow::anyhow!("internal error: database connection required for sync")
            })?;
            run_sync_once(&args.output, config, conn)
        }
    }
}

// ── one-shot sync ──────────────────────────────────────────────────────────

/// Runs a single pull → merge → push sync cycle and reports results.
///
/// # Errors
///
/// Returns an error if the provider cannot be constructed, the snapshot cannot
/// be taken, the sync cycle fails, or the merged state cannot be written back.
fn run_sync_once(
    output: &OutputFormat,
    config: &crate::config::Config,
    conn: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> anyhow::Result<()> {
    use directories::ProjectDirs;

    use crate::sync::{engine::SyncEngine, from_config, snapshot::StateSnapshot};

    let provider = from_config(config)?.ok_or_else(|| {
        anyhow::anyhow!("sync provider could not be constructed — is sync configured?")
    })?;

    // TODO(task-13): use a persisted, per-machine UUID instead of nil once
    // machine_id generation is wired into Config. machine_id is diagnostic
    // only and does not affect merge correctness.
    let local = StateSnapshot::from_db(conn, Uuid::nil())?;

    let sync_state_path = ProjectDirs::from("", "", "scribe").map_or_else(
        || std::path::PathBuf::from(".local/share/scribe/sync-state.json"),
        |d| d.data_dir().join("sync-state.json"),
    );

    let provider_name = format!("{:?}", config.sync.provider).to_lowercase();
    let engine = SyncEngine::new(provider, sync_state_path.clone(), provider_name.clone());

    let rt = tokio::runtime::Runtime::new()?;
    let mut state = SyncState::load(&sync_state_path);

    match rt.block_on(engine.run_once(local)) {
        Ok(merged) => {
            merged.write_to_db(conn)?;
            state.last_sync_at = Some(Utc::now());
            state.last_error = None;
            state.provider = Some(provider_name);
            state.save(&sync_state_path)?;
            match output {
                OutputFormat::Text => println!("Sync complete."),
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&state)?),
            }
        }
        Err(e) => {
            state.last_error = Some(e.to_string());
            let _ = state.save(&sync_state_path);
            anyhow::bail!("sync failed: {e}");
        }
    }
    Ok(())
}

// ── configure ─────────────────────────────────────────────────────────────

/// Configures the active sync provider, prompting for secrets interactively.
///
/// # Errors
///
/// Returns an error if prompting fails, keychain writes fail, or the config
/// cannot be saved.
fn run_configure(
    args: &SyncConfigureArgs,
    config: &mut crate::config::Config,
) -> anyhow::Result<()> {
    if args.remove {
        remove_secrets(config)?;
        println!("Sync secrets removed.");
        return Ok(());
    }

    let provider = match &args.provider {
        Some(p) => parse_provider_str(p)?,
        None => prompt_provider_choice()?,
    };

    config.sync.provider = provider;
    configure_provider(config)?;
    config.sync.enabled = true;
    config.save()?;

    println!(
        "Sync configured successfully. Provider: {:?}",
        config.sync.provider
    );
    Ok(())
}

/// Parses a provider name string into a `SyncProvider`.
///
/// # Errors
///
/// Returns an error if the string is not a recognised provider name.
fn parse_provider_str(s: &str) -> anyhow::Result<SyncProvider> {
    match s.to_lowercase().as_str() {
        "gist" => Ok(SyncProvider::Gist),
        "s3" => Ok(SyncProvider::S3),
        "icloud" => Ok(SyncProvider::ICloud),
        "jsonbin" => Ok(SyncProvider::JsonBin),
        "dropbox" => Ok(SyncProvider::Dropbox),
        "rest" => Ok(SyncProvider::Rest),
        "file" => Ok(SyncProvider::File),
        other => anyhow::bail!(
            "unknown provider '{other}'; valid values: gist, s3, icloud, jsonbin, dropbox, rest, file"
        ),
    }
}

/// Interactively prompts the user to select a sync provider.
///
/// # Errors
///
/// Returns an error if stdin cannot be read or the selection is out of range.
fn prompt_provider_choice() -> anyhow::Result<SyncProvider> {
    println!("Select a sync provider:");
    println!("  [1] GitHub Gist (recommended — free, requires GitHub account)");
    println!("  [2] S3-compatible (AWS S3, Cloudflare R2, MinIO, ...)");
    println!("  [3] iCloud Drive (macOS only)");
    println!("  [4] JSONBin.io (free tier)");
    println!("  [5] Dropbox");
    println!("  [6] Self-hosted REST (daemon-hosted master)");
    println!("  [7] Custom file path");
    print!("Choice [1-7]: ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    match line.trim() {
        "1" => Ok(SyncProvider::Gist),
        "2" => Ok(SyncProvider::S3),
        "3" => Ok(SyncProvider::ICloud),
        "4" => Ok(SyncProvider::JsonBin),
        "5" => Ok(SyncProvider::Dropbox),
        "6" => Ok(SyncProvider::Rest),
        "7" => Ok(SyncProvider::File),
        other => anyhow::bail!("invalid choice '{other}'; enter a number between 1 and 7"),
    }
}

/// Configures provider-specific settings and stores secrets in the keychain.
///
/// # Errors
///
/// Returns an error if prompting fails, keychain writes fail, or getrandom
/// fails during REST master secret generation.
fn configure_provider(config: &mut crate::config::Config) -> anyhow::Result<()> {
    match config.sync.provider {
        SyncProvider::Gist => configure_gist()?,
        SyncProvider::S3 => configure_s3(config)?,
        SyncProvider::ICloud => configure_icloud(config)?,
        SyncProvider::JsonBin => configure_jsonbin()?,
        SyncProvider::Dropbox => configure_dropbox()?,
        SyncProvider::Rest => configure_rest(config)?,
        SyncProvider::File => configure_file(config)?,
    }
    Ok(())
}

/// Configures the GitHub Gist sync provider.
///
/// # Errors
///
/// Returns an error if the PAT prompt fails or the keychain write fails.
fn configure_gist() -> anyhow::Result<()> {
    let token = rpassword::prompt_password("GitHub personal access token (PAT): ")?;
    KeychainStore::set("gist", "token", &token)
        .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
    println!("GitHub PAT stored in keychain.");
    Ok(())
}

/// Configures the S3-compatible sync provider.
///
/// # Errors
///
/// Returns an error if prompting fails or the keychain write fails.
fn configure_s3(config: &mut crate::config::Config) -> anyhow::Result<()> {
    let mut line = String::new();

    print!("S3 endpoint URL [https://s3.amazonaws.com]: ");
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut line)?;
    let endpoint = line.trim().to_owned();
    let endpoint = if endpoint.is_empty() {
        "https://s3.amazonaws.com".to_owned()
    } else {
        endpoint
    };
    line.clear();

    print!("Bucket name: ");
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut line)?;
    let bucket = line.trim().to_owned();
    line.clear();

    print!("Region [us-east-1]: ");
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut line)?;
    let region = line.trim().to_owned();
    let region = if region.is_empty() {
        "us-east-1".to_owned()
    } else {
        region
    };

    config.sync.s3.endpoint = endpoint;
    config.sync.s3.bucket = bucket;
    config.sync.s3.region = region;

    let access_key_id = rpassword::prompt_password("AWS access key ID: ")?;
    let secret_access_key = rpassword::prompt_password("AWS secret access key: ")?;

    KeychainStore::set("s3", "access_key_id", &access_key_id)
        .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
    KeychainStore::set("s3", "secret_access_key", &secret_access_key)
        .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;

    println!("S3 credentials stored in keychain.");
    Ok(())
}

/// Configures the iCloud Drive sync provider.
///
/// # Errors
///
/// Returns an error if prompting fails.
fn configure_icloud(config: &mut crate::config::Config) -> anyhow::Result<()> {
    let default = "~/Library/Mobile Documents/com~apple~CloudDocs/scribe-state.json";
    print!("iCloud Drive path [{default}]: ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let path = line.trim().to_owned();
    let path = if path.is_empty() {
        default.to_owned()
    } else {
        path
    };

    config.sync.icloud.path = std::path::PathBuf::from(path);
    println!("iCloud path configured.");
    Ok(())
}

/// Configures the JSONBin.io sync provider.
///
/// # Errors
///
/// Returns an error if the key prompt fails or the keychain write fails.
fn configure_jsonbin() -> anyhow::Result<()> {
    let key = rpassword::prompt_password("JSONBin.io access key: ")?;
    KeychainStore::set("jsonbin", "access_key", &key)
        .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
    println!("JSONBin.io access key stored in keychain.");
    Ok(())
}

/// Configures the Dropbox sync provider.
///
/// # Errors
///
/// Returns an error if the token prompt fails or the keychain write fails.
fn configure_dropbox() -> anyhow::Result<()> {
    println!("To generate a Dropbox access token:");
    println!("  1. Go to https://www.dropbox.com/developers/apps");
    println!("  2. Create a new app with 'Full Dropbox' access.");
    println!("  3. Under 'OAuth 2', click 'Generate access token'.");

    let token = rpassword::prompt_password("Dropbox access token: ")?;
    KeychainStore::set("dropbox", "access_token", &token)
        .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
    println!("Dropbox access token stored in keychain.");
    Ok(())
}

/// Configures the self-hosted REST sync provider.
///
/// # Errors
///
/// Returns an error if prompting fails, getrandom fails, or the keychain
/// write fails.
fn configure_rest(config: &mut crate::config::Config) -> anyhow::Result<()> {
    println!("Select REST role:");
    println!("  [1] Master");
    println!("  [2] Client");
    print!("Role [1-2]: ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    match line.trim() {
        "1" => configure_rest_master(config),
        "2" => configure_rest_client(config),
        other => anyhow::bail!("invalid choice '{other}'; enter 1 or 2"),
    }
}

/// Configures this machine as the REST sync master.
///
/// # Errors
///
/// Returns an error if entropy generation fails or the keychain write fails.
fn configure_rest_master(config: &mut crate::config::Config) -> anyhow::Result<()> {
    print!(
        "Listening port [{}]: ",
        crate::config::DEFAULT_REST_PORT_PUB
    );
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let port_str = line.trim().to_owned();
    let port: u16 = if port_str.is_empty() {
        crate::config::DEFAULT_REST_PORT_PUB
    } else {
        port_str
            .parse()
            .map_err(|_e| anyhow::anyhow!("invalid port number '{port_str}'"))?
    };

    config.sync.rest.port = port;
    config.sync.rest.role = crate::config::RestRole::Master;

    // Generate a 32-byte (256-bit) random secret and hex-encode it.
    // 32 bytes = 64 hex chars; this is sufficient for HMAC-SHA256 authentication.
    let mut secret_bytes = [0u8; 32];
    getrandom::fill(&mut secret_bytes)
        .map_err(|e| anyhow::anyhow!("failed to generate random secret: {e}"))?;
    let secret = hex::encode(secret_bytes);

    KeychainStore::set("rest", "secret", &secret)
        .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;

    println!("REST master configured on port {port}.");
    println!("Share this secret with client machines (shown once):");
    println!("  {secret}");
    Ok(())
}

/// Configures this machine as a REST sync client.
///
/// # Errors
///
/// Returns an error if prompting fails or the keychain write fails.
fn configure_rest_client(config: &mut crate::config::Config) -> anyhow::Result<()> {
    print!("Master server URL (e.g. http://192.168.1.1:7171): ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    line.trim().clone_into(&mut config.sync.rest.url);
    config.sync.rest.role = crate::config::RestRole::Client;

    let secret = rpassword::prompt_password("Shared secret: ")?;
    KeychainStore::set("rest", "secret", &secret)
        .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;

    println!("REST client configured.");
    Ok(())
}

/// Configures the local/network file sync provider.
///
/// # Errors
///
/// Returns an error if prompting fails.
fn configure_file(config: &mut crate::config::Config) -> anyhow::Result<()> {
    print!("File path for sync state document: ");
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    config.sync.file.path = std::path::PathBuf::from(line.trim());
    println!("File path configured.");
    Ok(())
}

/// Removes all keychain secrets for the currently configured provider.
///
/// # Errors
///
/// Returns an error if any keychain removal fails.
fn remove_secrets(config: &crate::config::Config) -> anyhow::Result<()> {
    match config.sync.provider {
        SyncProvider::Gist => {
            KeychainStore::remove("gist", "token")
                .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
        }
        SyncProvider::S3 => {
            KeychainStore::remove("s3", "access_key_id")
                .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
            KeychainStore::remove("s3", "secret_access_key")
                .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
        }
        SyncProvider::JsonBin => {
            KeychainStore::remove("jsonbin", "access_key")
                .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
        }
        SyncProvider::Dropbox => {
            KeychainStore::remove("dropbox", "access_token")
                .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
        }
        SyncProvider::Rest => {
            KeychainStore::remove("rest", "secret")
                .map_err(|e| anyhow::anyhow!("keychain error: {e}"))?;
        }
        SyncProvider::ICloud | SyncProvider::File => {}
    }
    Ok(())
}

// ── status ─────────────────────────────────────────────────────────────────

/// Shows the current sync status (provider, last sync time, last error).
///
/// # Errors
///
/// Returns an error if the sync state file cannot be read or output fails.
fn run_status(args: &SyncStatusArgs, config: &crate::config::Config) -> anyhow::Result<()> {
    let state_path = sync_state_path()?;
    let state = SyncState::load(&state_path);

    let provider_name = format!("{:?}", config.sync.provider).to_lowercase();
    let last_sync = state
        .last_sync_at
        .map_or_else(|| "never".to_owned(), |t| t.to_rfc3339());
    let last_error = state.last_error.as_deref().unwrap_or("none").to_owned();

    match args.output {
        OutputFormat::Text => {
            println!("sync enabled:  {}", config.sync.enabled);
            println!("provider:      {provider_name}");
            println!("last sync:     {last_sync}");
            println!("last error:    {last_error}");
        }
        OutputFormat::Json => {
            let obj = json!({
                "enabled":      config.sync.enabled,
                "provider":     provider_name,
                "last_sync_at": state.last_sync_at,
                "last_error":   state.last_error,
            });
            println!("{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

/// Returns the path to the sync-state JSON file.
///
/// Uses `$XDG_DATA_HOME/scribe/sync-state.json` or the platform default.
///
/// # Errors
///
/// Returns an error if the platform data directory cannot be determined.
fn sync_state_path() -> anyhow::Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "scribe")
        .ok_or_else(|| anyhow::anyhow!("could not determine platform data directory"))?;
    Ok(dirs.data_local_dir().join("sync-state.json"))
}
