# Scribe Sync — Design Spec

**Date:** 2026-04-02  
**Status:** Approved for implementation planning  

---

## Overview

Scribe gains an opt-in sync feature that allows the user's full state (projects,
tasks, todos, time entries, reminders, capture items) to be mirrored to an
online provider of their choice. State is shared across multiple machines by
pushing and pulling a JSON snapshot. Sync is off by default and activated
through `scribe setup` or `scribe sync configure`.

---

## Goals

- Allow a user to keep Scribe state in sync across multiple machines.
- Support multiple provider backends through a common trait interface.
- Secrets (API tokens, shared secrets) are stored in the OS keychain, never in
  plaintext config files.
- Sync happens automatically via the background daemon when the service is
  installed, and is available as a manual one-shot command regardless.
- Merge conflicts are resolved at the field level; last-write-wins is the
  tie-break based on `updated_at` timestamps already present on every entity.

## Non-Goals

- Real-time/streaming sync (push notifications, websockets).
- Syncing only a subset of entity types (all or nothing).
- Multiple simultaneously active providers (one active provider at a time).
- End-to-end encryption of the snapshot payload (secrets are protected; the
  payload itself is trusted to the provider's transport security).

---

## Architecture

### New Cargo feature: `sync`

All sync code — HTTP clients, keychain access, axum REST server — is gated
behind a single `sync` Cargo feature, mirroring the existing `mcp` feature
pattern. Pre-built release binaries include `sync` by default.

### Module layout

```
src/sync/
  mod.rs            — SyncProvider trait, re-exports
  engine.rs         — core sync logic: serialise, merge, push, pull
  snapshot.rs       — StateSnapshot struct (full serialisable state)
  keychain.rs       — OS keychain read/write abstraction
  providers/
    mod.rs
    gist.rs         — GitHub Gist
    s3.rs           — S3-compatible (AWS, Cloudflare R2, MinIO, etc.)
    icloud.rs       — iCloud Drive (file path on macOS)
    jsonbin.rs      — JSONBin.io
    dropbox.rs      — Dropbox API
    rest.rs         — Generic REST client (self-hosted master)
    file.rs         — Custom local/network file path

src/server/
  mod.rs            — axum + tokio HTTP server entry point
  handlers.rs       — GET /state, PUT /state
  auth.rs           — shared-secret Bearer token middleware
```

### SyncProvider trait

```rust
/// Abstraction over a remote sync backend.
#[async_trait::async_trait]
pub trait SyncProvider: Send + Sync {
    /// Upload the local state snapshot to the remote.
    async fn push(&self, snapshot: &StateSnapshot) -> Result<()>;

    /// Download the current remote snapshot.
    async fn pull(&self) -> Result<StateSnapshot>;
}
```

The trait is `async` because all network-backed providers (Gist, S3, JSONBin,
Dropbox, REST) require async I/O. The `file` and `icloud` providers implement
the trait with trivially async bodies wrapping `std::fs` calls. The `async_trait`
crate (or Rust's native async-in-traits once stabilised) is used to make the
trait object-safe.

Each provider module implements this trait. The `SyncEngine` in `engine.rs`
holds a `Box<dyn SyncProvider>` and is provider-agnostic.

---

## StateSnapshot

`StateSnapshot` is a flat, JSON-serialisable document containing every table.

```rust
pub struct StateSnapshot {
    pub snapshot_at:    DateTime<Utc>,
    pub machine_id:     Uuid,        // generated once on install, stored in config
    pub schema_version: u32,         // incremented on breaking snapshot shape changes
    pub projects:       Vec<Project>,
    pub tasks:          Vec<Task>,
    pub todos:          Vec<Todo>,
    pub time_entries:   Vec<TimeEntry>,
    pub reminders:      Vec<Reminder>,
    pub capture_items:  Vec<CaptureItem>,
}
```

`machine_id` is a UUID generated on first run and written to `config.toml`
under `[setup]`. It is used to attribute changes to a machine during merge
(for diagnostic purposes; it does not affect conflict resolution logic).

---

## Merge Algorithm

The merge runs on the local machine after pulling the remote snapshot, before
pushing the merged result back.

1. **Pull** the remote snapshot.
2. **For each remote entity** (keyed by `slug`):
   - If the entity does not exist locally → **insert** it.
   - If the entity exists locally and `remote.updated_at > local.updated_at` →
     **replace** the local record with the remote version (last-write-wins).
   - If the entity exists locally and `local.updated_at >= remote.updated_at` →
     **keep** the local record unchanged.
3. **For each local entity** not present in the remote snapshot:
   - It was created locally since the last sync → it will be included in the
     push in the next step.
4. **Push** the full merged local state as the new remote snapshot.

`archived_at` is treated as a regular field. If one machine archives a record,
the archive propagates to all machines on the next sync cycle.

**Idempotency:** Running sync twice in a row with no local changes produces no
remote write (push is skipped if the merged snapshot is byte-identical to the
pulled snapshot, compared by `snapshot_at` and a content hash).

---

## Configuration

Located at `~/.config/scribe/config.toml`. No secrets appear in this file.

```toml
[sync]
enabled = false
provider = "gist"      # active provider: gist | s3 | icloud | jsonbin | dropbox | rest | file
interval_secs = 60     # daemon sync interval in seconds

# Non-sensitive provider config. Secrets are stored in the OS keychain.

[sync.gist]
# Keychain entry: "scribe.sync.gist.token"
gist_id = ""           # populated automatically on first push

[sync.s3]
endpoint = ""          # e.g. https://s3.amazonaws.com or Cloudflare R2 URL
bucket = ""
key = "scribe-state.json"
region = "us-east-1"
# Keychain entries: "scribe.sync.s3.access_key_id", "scribe.sync.s3.secret_access_key"

[sync.icloud]
# macOS only. Scribe reads/writes this file path directly.
path = "~/Library/Mobile Documents/com~apple~CloudDocs/scribe-state.json"

[sync.jsonbin]
bin_id = ""            # populated automatically on first push
# Keychain entry: "scribe.sync.jsonbin.access_key"

[sync.dropbox]
path = "/scribe-state.json"
# Keychain entry: "scribe.sync.dropbox.access_token"

[sync.rest]
url = ""               # e.g. http://192.168.1.10:7171
role = "client"        # "master" | "client"
port = 7171            # only relevant when role = "master"
# Keychain entry: "scribe.sync.rest.secret"

[sync.file]
path = ""              # absolute path to a JSON file (e.g. Dropbox folder, NFS mount)
```

Only the section matching the active `provider` value is read at runtime.
Other sections may be present (to allow quick provider switching without
re-entering credentials) and are silently ignored.

---

## Secrets and Keychain

All provider secrets (API tokens, access keys, shared secrets) are stored in
the OS keychain under a well-known service name pattern:
`scribe.sync.<provider>.<field>`.

**Platform keychain backends:**

| Platform | Backend | Crate |
|---|---|---|
| macOS | macOS Keychain (via `security` framework) | `keyring` |
| Linux | `libsecret` (gnome-keyring / kwallet) | `keyring` |
| Windows | Windows Credential Manager | `keyring` |

The `keyring` crate provides a unified API across all three platforms.

**Linux headless behaviour:** If no keychain daemon is available on Linux
(i.e. `keyring` returns a `NoEntry` or service-unavailable error), Scribe
exits with a clear error message:

```
error: sync requires a keychain daemon to store secrets securely.
       Install and start gnome-keyring or kwallet, then re-run `scribe sync configure`.
```

No silent fallback to plaintext storage. The user is expected to resolve this
before sync can be configured.

**Secret lifecycle:**
- Written during `scribe sync configure` (interactive prompt, input hidden).
- Read at runtime by the `SyncEngine` before each push/pull cycle.
- Never written to `config.toml`, log files, or the snapshot payload.
- Removable via `scribe sync configure --remove` or directly from the OS
  keychain manager.

---

## CLI Commands

### `scribe sync`

One-shot manual sync. Reads config, loads the active provider, runs a single
push/pull cycle, prints a summary.

```
scribe sync [--output text|json]
```

### `scribe sync configure`

Interactive wizard to set the active provider and store secrets in the
keychain. Can also be reached from `scribe setup`.

```
scribe sync configure [--provider <name>]
scribe sync configure --remove   # clears keychain entries for the active provider
```

### `scribe sync status`

Displays the active provider, whether secrets are present in the keychain,
the timestamp of the last successful sync, and the next scheduled sync (if
the daemon is running).

```
scribe sync status [--output text|json]
```

---

## Daemon Integration

The existing daemon poll loop in `src/cli/daemon.rs` is extended with a second
interval ticker. The two timers run independently:

- **Reminder check** — every 30 seconds (unchanged).
- **Sync cycle** — every `sync.interval_secs` (default 60 seconds); only runs
  if `sync.enabled = true`.

### REST master server

When `sync.provider = "rest"` and `sync.rest.role = "master"`, the daemon
spawns an axum HTTP server as a background tokio task before entering the poll
loop. The server runs for the lifetime of the daemon process.

The server exposes two endpoints, both protected by a
`Authorization: Bearer <secret>` header (secret read from keychain):

| Method | Path | Description |
|---|---|---|
| `GET` | `/state` | Returns the current local snapshot as JSON |
| `PUT` | `/state` | Accepts a snapshot, merges it into local state, returns the merged snapshot |

Client machines configure `sync.rest.url` to point at the master's address and
`sync.rest.role = "client"`. The client's `SyncProvider::pull` calls
`GET /state`; `push` calls `PUT /state`.

The server only starts if the user has explicitly configured `role = "master"`
— it is never started implicitly. It binds to `0.0.0.0:<port>` by default;
`127.0.0.1` can be configured for local-only access.

---

## Provider Implementation Notes

### GitHub Gist (`gist`)

- Uses the GitHub REST API: `POST /gists` (create) and `PATCH /gists/{id}` (update).
- On first push, creates a new secret gist named `scribe-state.json` and
  writes the returned `gist_id` to `config.toml`.
- Pull: `GET /gists/{id}`, reads the `scribe-state.json` file content.
- Auth: `Authorization: Bearer <token>` (Personal Access Token with `gist` scope).
- No extra dependencies beyond `reqwest`.

### S3-compatible (`s3`)

- Single object at `s3://<bucket>/<key>`.
- Push: `PutObject`. Pull: `GetObject`.
- Auth: AWS SigV4 request signing.
- Endpoint is configurable to support Cloudflare R2, MinIO, and other
  S3-compatible stores.
- Dependency: `aws-sigv4` or a lightweight SigV4 implementation; `reqwest` for HTTP.

### iCloud Drive (`icloud`)

- No API — Scribe reads and writes a regular file at the configured path.
- iCloud Drive's sync daemon handles propagation between machines transparently.
- The file path defaults to the user's iCloud Drive folder but is configurable.
- No network code; uses `std::fs`. No additional dependencies.
- macOS only (documented as such; the option is hidden on other platforms).

### JSONBin.io (`jsonbin`)

- Free-tier JSON storage API. On first push, creates a new bin and writes the
  `bin_id` to `config.toml`.
- Push: `PUT /b/{id}`. Pull: `GET /b/{id}/latest`.
- Auth: `X-Access-Key` header (stored in keychain).

### Dropbox (`dropbox`)

- Uses the Dropbox API v2: `files/upload` (push) and `files/download` (pull).
- Auth: OAuth2 access token (stored in keychain).
- A helper note in `scribe sync configure` guides the user to generate a token
  via the Dropbox App Console (no OAuth redirect flow needed — long-lived token).

### REST (`rest`)

- Client role: `GET <url>/state` (pull) and `PUT <url>/state` (push).
- Master role: axum server as described in Daemon Integration above.
- Auth: `Authorization: Bearer <secret>` (same secret configured on both sides).

**Secret provisioning:**

When configuring `role = "master"`, `scribe sync configure` auto-generates a
cryptographically random secret (32 bytes, hex-encoded), stores it in the
keychain, and prints it exactly once:

```
REST sync master configured on port 7171.

Share this secret with every client machine — it will not be shown again:

  SECRET: 4a9f2c8e1b7d3f6a...

On each client machine run:
  scribe sync configure --provider rest --role client
  and enter the URL and secret when prompted.
```

On client machines, `scribe sync configure --provider rest --role client`
prompts for the master URL and the secret (input hidden), then stores the
secret in the client's keychain. The secret is never written to `config.toml`
on either side.

### File (`file`)

- Reads and writes a local file path (absolute). No network code.
- Useful for Dropbox/OneDrive/Syncthing managed folders where a third-party
  daemon handles remote propagation.
- No additional dependencies.

---

## Error Handling

- All sync errors are non-fatal from the daemon's perspective. A failed sync
  cycle logs the error (via `tracing`) and is retried on the next interval.
- The timestamp of the last successful sync and the last error (if any) are
  stored in a lightweight state file at
  `~/.local/share/scribe/sync-state.json` (not in SQLite — avoids polluting the
  synced dataset with sync metadata). Its shape:
  ```json
  {
    "last_sync_at": "2026-04-02T09:01:00Z",
    "last_error": null,
    "provider": "gist"
  }
  ```
- `scribe sync status` reads this file to report current state.
- Provider errors (e.g. 401 Unauthorized, network timeout) surface as
  actionable messages: `sync failed: GitHub API returned 401 — check your token with 'scribe sync configure'`.

---

## Testing Strategy

- **Unit tests** for `engine.rs` merge logic using in-memory `StateSnapshot`
  fixtures. Covers: remote-only entity, local-only entity, remote newer,
  local newer, identical timestamps, archived propagation.
- **Provider tests** use a `MockProvider` that implements `SyncProvider` and
  records calls. No real network calls in unit tests.
- **Integration tests** for the REST server: spin up the axum server on a
  random port, run push/pull cycles, assert merged state.
- The `file` and `icloud` providers are tested with `tempfile` directories.

---

## Rollout Considerations

- `sync` feature is off by default in `config.toml` (`enabled = false`).
- Existing users are unaffected until they run `scribe sync configure`.
- `scribe setup` gains a new optional sync step after daemon installation.
- The `sync` feature is included in pre-built release binaries alongside `mcp`.
- `schema_version` in the snapshot allows future migration of the payload
  format without breaking existing synced state.
