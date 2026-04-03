# Scribe

<p align="center">
  <img src=".github/assets/scribe-logo.png" alt="Scribe logo" width="400" />
</p>

[![Build status](https://github.com/johan-steffens/scribe/actions/workflows/release.yml/badge.svg)](https://github.com/johan-steffens/scribe/actions)
[![Version](https://img.shields.io/badge/version-1.0.4-blue)](https://github.com/johan-steffens/scribe/releases)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)](https://www.rust-lang.org)
[![MIT license](https://img.shields.io/badge/license-MIT-green)](LICENSE)

A keyboard-driven CLI and TUI for managing projects, tasks, todos, time
tracking, reminders, and quick captures — all in a single SQLite file you
own. No accounts, no cloud lock-in, no monthly fees. Sync across machines
when you want to.

---

## Why Scribe?

- **Everything in one place** — not five different apps held together with
  Zapier. Projects, tasks, todos, time entries, reminders, and a quick-capture
  inbox.
- **Your data stays yours** — single SQLite file. Copy it, back it up,
  open it in any SQLite browser. No proprietary formats.
- **Works offline** — full functionality without an internet connection.
- **Sync when you want it** — GitHub Gist, S3, iCloud, Dropbox, or your own
  self-hosted server. Secrets live in your OS keychain.
- **Built for keyboard warriors** — scriptable CLI for automation, full TUI for
  interactive work, shell completions for fast input.
- **AI agent-ready** — MCP server exposes your data as tools to coding agents
  (Claude Code, Cursor, OpenCode, etc.).

---

## Quick Start

```sh
# Install (pre-built binary or from source)
curl -Lo scribe https://github.com/johan-steffens/scribe/releases/latest/download/scribe-macos-aarch64
chmod +x scribe && sudo mv scribe /usr/local/bin/

# Or from source: cargo install --path . --features mcp,sync

# Run the setup wizard
scribe setup --wizard

# Open the TUI
scribe

# Or use the CLI directly
scribe task add "Review PR" --priority high --project myproject
scribe track start --task mytask
scribe capture "Remember to call mom"
```

---

## Core Concepts

**Projects** group related work. **Tasks** have priority, status, and due dates.
**Todos** are lightweight checklists. **Time tracking** links timers to tasks
or projects. **Reminders** fire desktop notifications at scheduled times.
**Capture** grabs fleeting thoughts into the inbox for processing later.

Everything has slugs (`my-task-20260403`) for fast CLI and completion lookup.

---

## TUI

Run `scribe` (no arguments) to open the full-screen interface:

| Key | Action |
|-----|--------|
| `d/p/t/o/r/i/m` | Switch views (Dashboard/Projects/Tasks/Todos/Tracker/Inbox/Reminders) |
| `n` | New item |
| `e` | Edit selected |
| `Space` | Toggle done / start timer |
| `?` | Help |
| `q` | Quit |

---

## CLI Overview

```sh
scribe project add myproject --name "My Project"
scribe task add "Build feature" --project myproject --priority high
scribe todo add "Review PRs" --project myproject
scribe track start --task mytask
scribe track report --week
scribe capture "Fix that bug later"
scribe reminder add --project myproject --at "tomorrow 9am"
scribe inbox process <slug>
scribe sync configure --provider gist
```

Every subcommand supports `--output json` for scripting.

---

## Installation

### Pre-built binary (recommended)

```sh
# macOS ARM64
curl -Lo scribe https://github.com/johan-steffens/scribe/releases/latest/download/scribe-macos-aarch64

# macOS Intel
curl -Lo scribe https://github.com/johan-steffens/scribe/releases/latest/download/scribe-macos-x86_64

# Linux
curl -Lo scribe https://github.com/johan-steffens/scribe/releases/latest/download/scribe-linux-x86_64

chmod +x scribe && sudo mv scribe /usr/local/bin/
```

Binaries include MCP server and sync features. Verify against checksums in
`SHA256SUMS.txt` on the releases page.

### From source

Requires Rust 1.85+ (edition 2024).

```sh
cargo install --path . --features mcp,sync
```

---

## Background Daemon

For continuous reminder notifications and auto-sync, install the daemon service:

```sh
scribe setup --wizard   # or:
scribe service install  # install only

# Daemon runs in background, auto-syncs every 60s if sync is enabled
scribe daemon restart   # restart after config changes
scribe daemon reinstall # reinstall after upgrades
```

---

## Configuration

`~/.config/scribe/config.toml` (all settings optional):

```toml
[sync]
enabled = true
provider = "gist"       # gist | s3 | icloud | jsonbin | dropbox | rest | file
interval_secs = 60

[notifications]
enabled = true

[display]
date_format = "%Y-%m-%d"
time_format = "%H:%M"
```

Secrets (tokens, keys) go in the **OS keychain**, never the config file.

---

## Sync

Sync mirrors your full state to a provider of your choice. Off by default.

```sh
scribe sync configure --provider gist
# Follow the prompts (GitHub token stored in keychain)

# Or self-host the REST master (one machine = master, others = clients)
scribe sync configure --provider rest
```

| Provider | Notes |
|---|---|
| `gist` | GitHub Gist — free, recommended |
| `s3` | AWS S3, Cloudflare R2, MinIO, any S3-compatible store |
| `icloud` | iCloud Drive (macOS, no credentials) |
| `rest` | Self-hosted master server via `scribe daemon` |
| `file` | Local/network path (Dropbox, NFS, Syncthing) |

---

## AI Agent Integration

**Skill files** teach agents Scribe's commands. Run once:

```sh
scribe agent install
```

**MCP server** (build with `mcp` feature) exposes all data as tools:

```sh
cargo install --path . --features mcp
# Copy the printed MCP config snippet to your agent's config
```

---

## Data

Your data lives at `~/.local/share/scribe/scribe.db`. Backup with `cp`, inspect
with `sqlite3`, move with `db_path` in config.

---

## License

MIT
