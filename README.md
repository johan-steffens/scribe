# Scribe

<p align="center">
  <img src=".github/assets/scribe-logo.png" alt="Scribe logo" width="250" />
</p>

Scribe is a personal productivity tool for the terminal. It keeps projects,
tasks, todos, time entries, a quick-capture inbox, and reminders in a single
local SQLite database, and exposes them through both a scriptable CLI and a
keyboard-driven full-screen TUI — no accounts, no network, no background
services.

---

## Features

- **Projects** — group all work into named projects with status tracking.
- **Tasks** — priority levels (`low`, `medium`, `high`, `urgent`), status
  (`todo`, `in_progress`, `done`, `cancelled`), and optional due dates.
- **Todos** — lightweight checklist items; simpler than tasks, no priority or
  due date.
- **Time tracking** — start/stop timers linked to tasks or projects; daily and
  weekly reports.
- **Quick capture** — dump a thought into the inbox with one command; process
  it later.
- **Reminders** — scheduled reminders checked on every startup, with flexible
  datetime input.
- **TUI** — full-screen keyboard-driven interface (`scribe` with no arguments).
- **CLI** — every operation is also a scriptable subcommand.
- **Tab completion** — planned for a future release; not yet available.
- **Machine-readable output** — every subcommand accepts `--output json`.
- **Offline-first** — all data is in a single SQLite file you own.

---

## Quick Start

```sh
# 1. Install from source
cargo install --path .

# 2. Create a project and a task
scribe project add payments --name "Payments Integration"
scribe task add "Fix login bug" --project payments --priority high

# 3. Open the TUI
scribe
```

---

## Installation

### From source

```sh
git clone https://github.com/your-org/scribe.git
cd scribe
cargo install --path .
```

Requires Rust 1.77 or later (stable).

### Shell completions

Shell completion support (static flag/subcommand completions via `clap_complete`
plus live slug lookup) is planned for a future release and is not yet available.

---

## Configuration

The configuration file lives at `~/.config/scribe/config.toml`
(or `$XDG_CONFIG_HOME/scribe/config.toml`). The file is optional — if it does
not exist all defaults apply.

```toml
[data]
# Override the database path. Leave empty or omit to use the default.
db_path = ""

[notifications]
# Whether to fire desktop notifications for due reminders (Phase 5).
enabled = true

[display]
# strftime-compatible date and time format strings used in output.
date_format = "%Y-%m-%d"
time_format = "%H:%M"
```

| Key | Default | Description |
|-----|---------|-------------|
| `data.db_path` | `~/.local/share/scribe/scribe.db` | Override the database file path. |
| `notifications.enabled` | `true` | Enable desktop notifications for fired reminders. |
| `display.date_format` | `%Y-%m-%d` | `strftime` format used when displaying dates. |
| `display.time_format` | `%H:%M` | `strftime` format used when displaying times. |

---

## Data storage

The SQLite database is stored at `~/.local/share/scribe/scribe.db`
(or `$XDG_DATA_HOME/scribe/scribe.db`).

- **Backup**: copy the file — `cp ~/.local/share/scribe/scribe.db ~/backups/`.
- **Inspect**: open with any SQLite browser (e.g. `sqlite3`, DB Browser for
  SQLite) — the schema is straightforward and human-readable.
- **Override path**: set `data.db_path` in `config.toml` or point
  `SCRIBE_TEST_DB` to an alternate path (used by integration tests).

---

## TUI key bindings reference

### Global (all views)

| Key | Action |
|-----|--------|
| `d` | Switch to Dashboard |
| `p` | Switch to Projects |
| `t` | Switch to Tasks |
| `o` | Switch to Todos |
| `r` | Switch to Tracker |
| `i` | Switch to Inbox |
| `m` | Switch to Reminders |
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `/` | Enter filter mode |
| `Esc` | Clear error / close help / exit filter |
| `?` | Toggle help overlay |
| `q` / `Ctrl-C` | Quit |

### Normal mode (list views)

| Key | Action |
|-----|--------|
| `n` | New item |
| `e` | Edit selected item |
| `D` | Archive / delete selected item (confirmation required) |
| `Space` | Toggle done (todos) / start or stop timer (tracker) |
| `Enter` | Process selected item (inbox) |
| `v` | Move selected todo to another project (Todos view only) |

### Filter mode

| Key | Action |
|-----|--------|
| Any character | Append to filter string |
| `Backspace` | Remove last character |
| `Enter` | Confirm filter, return to normal mode |
| `Esc` | Clear filter, return to normal mode |

### Forms and confirmation dialogs

| Key | Action |
|-----|--------|
| `Tab` | Next field |
| `Shift-Tab` | Previous field |
| `Enter` | Submit form |
| `Esc` | Cancel |
| `y` / `Enter` | Confirm dialog |
| `n` / `Esc` | Cancel dialog |

---

## CLI quick reference

```
scribe
├── project
│   ├── add     <slug> --name <name> [--desc <text>] [--output json]
│   ├── list    [--status active|paused|completed] [--archived] [--output json]
│   ├── show    <slug> [--output json]
│   ├── edit    <slug> [--new-slug <s>] [--name <n>] [--desc <t>] [--status <s>] [--output json]
│   ├── archive <slug> [--output json]
│   ├── restore <slug> [--output json]
│   └── delete  <slug> [--output json]
├── task
│   ├── add     <title> [--project <slug>] [--priority low|medium|high|urgent] [--due YYYY-MM-DD] [--output json]
│   ├── list    [--project <slug>] [--status <s>] [--priority <p>] [--archived] [--output json]
│   ├── show    <slug> [--output json]
│   ├── edit    <slug> [--title <t>] [--status <s>] [--priority <p>] [--due <d>] [--output json]
│   ├── move    <slug> --project <slug> [--output json]
│   ├── done    <slug> [--output json]
│   ├── archive <slug> [--output json]
│   ├── restore <slug> [--output json]
│   └── delete  <slug> [--output json]
├── todo
│   ├── add     <title> [--project <slug>] [--output json]
│   ├── list    [--project <slug>] [--all] [--archived] [--output json]
│   ├── show    <slug> [--output json]
│   ├── move    <slug> --project <slug> [--output json]
│   ├── done    <slug> [--output json]
│   ├── archive <slug> [--output json]
│   ├── restore <slug> [--output json]
│   └── delete  <slug> [--output json]
├── track
│   ├── start   [--task <slug>] [--project <slug>] [--note <text>] [--output json]
│   ├── stop    [--output json]
│   ├── status  [--output json]
│   └── report  [--today] [--week] [--project <slug>] [--output json]
├── capture     <text> [--output json]
├── inbox
│   ├── list    [--all] [--output json]
│   └── process <slug> [--output json]
└── reminder
    ├── add     --project <slug> --at <datetime> [--task <slug>] [--message <text>] [--output json]
    ├── list    [--project <slug>] [--archived] [--output json]
    ├── show    <slug> [--output json]
    ├── archive <slug> [--output json]
    ├── restore <slug> [--output json]
    └── delete  <slug> [--output json]
```

Running `scribe` with no subcommand opens the TUI.

---

## Contributing / Development

```sh
# Build
cargo build

# Run tests
cargo test

# Lint (zero warnings required)
cargo clippy -- -D warnings

# Format
cargo fmt
```

The project targets `cargo clippy -- -D warnings` passing on every commit.

---

## License

MIT
