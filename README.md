# Scribe

<p align="center">
  <img src=".github/assets/scribe-logo.png" alt="Scribe logo" width="400" />
</p>

<p align="center">
  <a href="https://github.com/johan-steffens/scribe/actions"><img src="https://github.com/johan-steffens/scribe/actions/workflows/release.yml/badge.svg" alt="Build status"></a>
  <a href="https://github.com/johan-steffens/scribe/releases"><img src="https://img.shields.io/github/v/release/johan-steffens/scribe" alt="Version"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.85%2B-orange" alt="Rust"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green" alt="MIT license"></a>
</p>

A keyboard-driven CLI and TUI for managing projects, tasks, todos, time
tracking, reminders, and quick captures — all in a single SQLite file you
own. No accounts, no cloud lock-in, no monthly fees. Sync across machines
when you want to.

Scribe is **AI-ready** — connect it to your coding agent and manage your
productivity through conversation. Ask your agent to add tasks, review your
tracker, process your inbox, or schedule reminders using plain English.

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
- **Well-tested** — 80%+ code coverage via `cargo-llvm-cov`. Integration tests for
  TUI (using `ratatui` `TestBackend`) and sync (using `wiremock`).

---

## Quick Start

```sh
# Install (Homebrew recommended)
brew tap johan-steffens/scribe
brew install scribe

# Or pre-built binary
curl -Lo scribe https://github.com/johan-steffens/scribe/releases/latest/download/scribe-macos-aarch64
chmod +x scribe && sudo mv scribe /usr/local/bin/

# Run the setup wizard
```

---

## Core Concepts

**Projects** group related work. **Tasks** have priority, status, and due dates.
**Todos** are lightweight checklist items (stored as tasks with `kind = 'checklist_item'`).
**Time tracking** links timers to tasks or projects. **Reminders** fire desktop
notifications at scheduled times. **Capture** grabs fleeting thoughts into the
inbox for processing later.

Everything has slugs (`my-task-20260403`) for fast CLI and completion lookup.

### Task Hierarchy (Sub-tasks)

Tasks support infinite hierarchical nesting via an optional `parent_id`. Use
`--parent` to create sub-tasks:

```sh
# Create a parent task
scribe task add "Build new feature"

# Create a sub-task under it
scribe task add "Write tests" --parent build-new-feature
```

Sub-tasks inherit the project from their parent. Use `scribe task toggle <slug>`
to quickly check off items (toggles done/undone for both tasks and todos).

---

## AI Agent Integration

Connect Scribe to your AI agent to manage your productivity through conversation.

**Skill files** teach agents Scribe's commands. Run once:

```sh
scribe agent install
```

**MCP server** (build with `mcp` feature) exposes all data as tools:

```sh
cargo install --path . --features mcp
# Copy the printed MCP config snippet to your agent's config
```

### Talk to Your Agent

Once connected, you can manage your life with plain English:

```
"You: Add a high priority task to review the API design doc"
Agent: → scribe task add "Review API design doc" --priority high

"You: What should I be working on today?"
Agent: → scribe task list --status todo
     → Shows your tasks due today or overdue

"You: Start tracking time on the backend project"
Agent: → scribe track start --project backend

"You: Remind me to call mom tomorrow at 2pm"
Agent: → scribe reminder add --at "tomorrow 2pm" --message "Call mom"

"You: Process my inbox"
Agent: → scribe inbox list
     → Walks through unprocessed captures, converts to tasks/todos

"You: How did I spend my time this week?"
Agent: → scribe track report --week
     → Shows time entries grouped by project

"You: Sync everything to gist"
Agent: → scribe sync push
```

Your agent acts as a smart interface — it knows your projects, tasks, and
workflow. No more context-switching between apps.

---

## Knowledge Management

Scribe includes a built-in **Personal Knowledge Management (PKM)** system for
long-form notes with automatic cross-linking.

### Notes

Notes are Markdown documents stored directly in your SQLite database. Unlike
tasks and todos, note slugs are user-defined (not auto-prefixed by project).

Notes can be managed from the **CLI**, **TUI**, or **MCP**:

```sh
scribe note add "Architecture draft"          # open $EDITOR
scribe note create --title "..." --content "..."
scribe note list
scribe note show arch-draft
scribe note edit arch-draft
```

In the TUI, press `m` for the Notes view, then `n` / `e` to create or edit.
AI agents use MCP tools: `read_note`, `write_note`, and `search_notes`.

### Wiki-style Links

Use `[[slug]]` syntax to link notes to other notes, tasks, or projects:

| Link syntax | Resolves to |
|-------------|-------------|
| `[[payments-task-fix-login]]` | A task |
| `[[payments]]` | A project |
| `[[my-design-doc]]` | Another note |

Links are automatically parsed on save and tracked in the `links` table. This
lets you build a graph of connections between your knowledge base and your
work.

### Full-text Search

Notes are indexed with SQLite FTS5. AI agents can search your knowledge base
via the `search_notes` MCP tool, which returns up to 50 matching notes ordered
by relevance (BM25 ranking).

---

## TUI

Run `scribe` (no arguments) to open the full-screen interface:

| Key | Action |
|-----|--------|
| `d/p/t/o/n/r/i/m` | Switch views (Dashboard/Projects/Tasks/Todos/**Notes**/Tracker/Inbox/Reminders) |
| `N` | New item (Shift+n — `n` alone is the Notes view) |
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
scribe report project myproject
scribe sync configure --provider gist
```

Every subcommand supports `--output json` for scripting.

---

## Reporting

Generate reports across all Scribe domains:

```sh
# Summary report (all domains)
scribe report

# Project-specific report
scribe report project myproject

# Task-specific report
scribe report task myproject-task-fix-login

# Time tracking report
scribe report track --week

# Filter by time window
scribe report --today
scribe report --week
```

All reports support `--output json` for machine-readable output and `--detailed` for expanded information.

---

## Installation

### Homebrew (macOS & Linux)

```sh
brew tap johan-steffens/scribe
brew install scribe
```

### Pre-built binary

```sh
# macOS ARM64
curl -Lo scribe https://github.com/johan-steffens/scribe/releases/latest/download/scribe-macos-aarch64
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
scribe service status   # check if daemon is running
scribe service restart  # restart after config changes
scribe service reinstall # reinstall after upgrades
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

## Data

Your data lives at `~/.local/share/scribe/scribe.db`. Backup with `cp`, inspect
with `sqlite3`, move with `db_path` in config.

---

## License

MIT
