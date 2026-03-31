# Scribe — Detailed Project Plan

**Guideline compliance date: 2026-02-21**
All code in this project must follow the `rust-development` skill
(`~/.claude/skills/rust-development`) before any `.rs` file is written or modified.

---

## Vision

Scribe is a personal productivity command-line tool written in Rust.
It serves as a single, offline-first hub for:

- **Work / project management** — projects, milestones, and tasks with statuses, priorities, and due dates
- **Time tracking** — start/stop timers linked to tasks or projects, with historical reports
- **Todo list tracking** — lightweight todos that may or may not belong to a project
- **Quick capture** — one-keystroke/one-command inbox to dump thoughts without context switching
- **Reminders** — scheduled notifications surfaced in the TUI and/or via system notifications

The tool is available in two modes:

| Mode | Description |
|------|-------------|
| **CLI** | Scriptable, pipe-friendly subcommands (`scribe task add`, `scribe track start`, …) |
| **TUI** | Full-screen, keyboard-driven terminal UI with multiple panes |

---

## Design Principles

1. **Offline-first, local data** — all data lives in a single SQLite file (via `rusqlite`).
   No network calls unless the user opts in to sync (future scope).
2. **Single binary** — `cargo install scribe` → done.
3. **Composable** — every action that can be done in the TUI can also be done via CLI.
4. **No magic** — data can be inspected and backed up; SQLite file is human-readable with any SQLite browser.
5. **Fast** — sub-100 ms startup, sub-16 ms TUI frame rendering on commodity hardware.
6. **Well documented** — every public API has rustdoc following M-CANONICAL-DOCS; architecture decisions live in `plans/`.

---

## Technology Stack

| Concern | Crate | Reason |
|---------|-------|--------|
| CLI argument parsing | `clap` (derive feature) | Ergonomic, generates help automatically |
| Shell completions | `clap_complete` + dynamic hidden subcommand | Static flag/subcommand + live DB value completion |
| TUI rendering | `ratatui` | Actively maintained successor to `tui-rs` |
| TUI event loop | `crossterm` | Cross-platform, used by ratatui |
| Persistence | `rusqlite` (bundled feature) | Single-file DB, zero-dependency deploy |
| Migrations | `rusqlite_migration` | Lightweight, embedded SQL migrations |
| Date/time | `chrono` | De-facto standard |
| Error handling (app) | `anyhow` | App-boundary errors (M-APP-ERROR) |
| Serialization | `serde` + `serde_json` | Config file and export |
| Configuration | `directories` + `toml` | XDG-compliant config path |
| Notifications | `notify-rust` (feature-gated) | Desktop notifications |
| Logging | `tracing` + `tracing-subscriber` | Structured, filterable (M-LOG-STRUCTURED) |
| Allocator | `mimalloc` | Global allocator for performance gains (M-MIMALLOC-APPS) |
| Testing | `rstest` | Parametric test helpers |

Note: no separate ORM — raw `rusqlite` queries keep the dependency tree small and
third-party types from leaking through public APIs (M-DONT-LEAK-TYPES).

---

## Repository Layout

```
scribe/
├── Cargo.toml                   # Includes required lint tables (M-STATIC-VERIFICATION)
├── plans/
│   ├── PLAN.md                  # This file
│   ├── ARCHITECTURE.md          # Module interaction diagrams
│   └── DATA_MODEL.md            # Entity definitions and SQL schema
├── src/
│   ├── main.rs                  # Entry point: dispatch CLI vs TUI; sets mimalloc allocator
│   ├── cli/
│   │   ├── mod.rs               # CLI root: Clap App definition
│   │   ├── project.rs           # `scribe project` subcommands
│   │   ├── task.rs              # `scribe task` subcommands
│   │   ├── todo.rs              # `scribe todo` subcommands
│   │   ├── track.rs             # `scribe track` subcommands
│   │   ├── capture.rs           # `scribe capture` subcommand
│   │   └── reminder.rs          # `scribe reminder` subcommands
│   ├── tui/
│   │   ├── mod.rs               # TUI entry point, event loop
│   │   ├── app.rs               # Application state machine
│   │   ├── ui.rs                # Layout and widget composition
│   │   ├── views/
│   │   │   ├── dashboard.rs     # Home screen: today's tasks + active timer
│   │   │   ├── projects.rs      # Project list / detail
│   │   │   ├── tasks.rs         # Task list with filters
│   │   │   ├── todos.rs         # Todo list
│   │   │   ├── tracker.rs       # Time tracking history + active timer
│   │   │   ├── inbox.rs         # Quick-capture inbox
│   │   │   └── reminders.rs     # Reminder list
│   │   └── components/
│   │       ├── table.rs         # Reusable sortable table widget
│   │       ├── form.rs          # Generic form/input widget
│   │       └── status_bar.rs    # Bottom status bar
│   ├── db/
│   │   ├── mod.rs               # DB connection and migrations runner
│   │   ├── migrations.rs        # Embedded SQL migrations
│   │   └── schema.sql           # Canonical SQL schema (documentation only)
│   ├── domain/
│   │   ├── mod.rs               # Re-exports with #[doc(inline)] (M-DOC-INLINE)
│   │   ├── project.rs           # Project entity + repository trait
│   │   ├── task.rs              # Task entity + repository trait
│   │   ├── todo.rs              # Todo entity + repository trait
│   │   ├── time_entry.rs        # TimeEntry entity + repository trait
│   │   ├── capture.rs           # CaptureItem entity + repository trait
│   │   └── reminder.rs          # Reminder entity + repository trait
│   ├── store/                   # Concrete SQLite repository implementations
│   │   ├── mod.rs
│   │   ├── project_store.rs     # SqliteProjects implements Projects trait
│   │   ├── task_store.rs        # SqliteTasks implements Tasks trait
│   │   ├── todo_store.rs        # SqliteTodos implements Todos trait
│   │   ├── time_entry_store.rs  # SqliteTimeEntries implements TimeEntries trait
│   │   ├── capture_store.rs     # SqliteCaptureItems implements CaptureItems trait
│   │   └── reminder_store.rs    # SqliteReminders implements Reminders trait
│   ├── ops/                     # Business logic (replaces "service/" — avoids weasel word M-CONCISE-NAMES)
│   │   ├── mod.rs
│   │   ├── projects.rs          # Project operations (was ProjectService)
│   │   ├── tasks.rs             # Task operations (was TaskService)
│   │   ├── todos.rs             # Todo operations (was TodoService)
│   │   ├── tracker.rs           # Timer start/stop, duration computation
│   │   ├── inbox.rs             # Inbox / quick-capture management
│   │   └── reminders.rs         # Reminder scheduling / triggering
│   └── config.rs                # Config file loading (XDG path)
└── tests/
    ├── cli_integration.rs       # End-to-end CLI tests via `assert_cmd`
    └── ops_tests.rs             # Operations-layer tests with in-memory DB
```

---

## Data Model (High Level)

See `plans/DATA_MODEL.md` for the full schema and design rationale.

### Identifier Model

Every entity (project, task, todo, time entry, reminder, capture item) is
identified by a **globally unique slug** — never a raw numeric ID in the CLI or
TUI. Numeric primary keys exist in the DB for relational integrity only and are
never exposed to the user.

#### Slug formats

| Entity | Format | Example |
|--------|--------|---------|
| Project | user-defined kebab-case | `payments` |
| Task | `{project}-task-{title-slug}[-{4-char suffix on collision}]` | `payments-task-fix-login` |
| Todo | `{project}-todo-{title-slug}[-{suffix}]` | `payments-todo-review-docs` |
| Reminder | `{project}-reminder-{title-slug}[-{suffix}]` | `payments-reminder-deploy-friday` |
| TimeEntry | `{project}-entry-{YYYYMMDD}-{HHmmss}[-{suffix}]` | `payments-entry-20260331-143000` |
| CaptureItem | `capture-{YYYYMMDD}-{HHmmss}[-{suffix}]` | `capture-20260331-143000` |

Auto-generated slugs are derived by lowercasing the title, stripping
non-alphanumeric characters, collapsing spaces/punctuation to `-`, and
truncating to a reasonable length. A 4-character random alphanumeric suffix is
appended only when a collision is detected.

### Binding Model

Everything is always bound to a project — `project_id` is `NOT NULL` on all
entities except `CaptureItem` (which is unstructured inbox by design).

A system-reserved project with `slug = "quick-capture"` is seeded on first run.
When the user creates an item without specifying a project, it lands here
automatically. Items can be recategorised at any time by updating `project_id`.

The reserved project cannot be deleted or archived (`is_reserved = 1`).

### Archive Model

There are **no hard deletes** for user data. Every entity has an `archived_at`
timestamp (`NULL` = active). When a project is deleted:

1. The user is asked whether to archive all items in the project.
2. On confirmation, all linked tasks, todos, time entries, and reminders receive
   `archived_at = NOW()`. The project itself is also archived.
3. Archived items are hidden from all default views and accessible via an
   Archives screen.
4. Any item or project can be restored (un-archived) at any time.

### Projects

```
projects
  id:          INTEGER PRIMARY KEY
  slug:        TEXT NOT NULL UNIQUE       -- e.g. "payment-automation"
  name:        TEXT NOT NULL
  description: TEXT
  status:      TEXT NOT NULL              -- "active" | "paused" | "completed"
  is_reserved: INTEGER NOT NULL DEFAULT 0 -- 1 = cannot be deleted/archived
  archived_at: TEXT                       -- NULL = active; ISO 8601
  created_at:  TEXT NOT NULL
  updated_at:  TEXT NOT NULL
```

### Tasks

```
tasks
  id:          INTEGER PRIMARY KEY
  slug:        TEXT NOT NULL UNIQUE       -- e.g. "payments-task-fix-login"
  project_id:  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT
  title:       TEXT NOT NULL
  description: TEXT
  status:      TEXT NOT NULL              -- "todo" | "in_progress" | "done" | "cancelled"
  priority:    TEXT NOT NULL              -- "low" | "medium" | "high" | "urgent"
  due_date:    TEXT                       -- ISO 8601 date; nullable
  archived_at: TEXT
  created_at:  TEXT NOT NULL
  updated_at:  TEXT NOT NULL
```

### Todos

```
todos
  id:          INTEGER PRIMARY KEY
  slug:        TEXT NOT NULL UNIQUE       -- e.g. "payments-todo-review-docs"
  project_id:  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT
  title:       TEXT NOT NULL
  done:        INTEGER NOT NULL DEFAULT 0 -- boolean
  archived_at: TEXT
  created_at:  TEXT NOT NULL
  updated_at:  TEXT NOT NULL
```

### TimeEntries

```
time_entries
  id:          INTEGER PRIMARY KEY
  slug:        TEXT NOT NULL UNIQUE       -- e.g. "payments-entry-20260331-143000"
  project_id:  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT
  task_id:     INTEGER REFERENCES tasks(id) ON DELETE SET NULL  -- optional
  started_at:  TEXT NOT NULL              -- ISO 8601
  ended_at:    TEXT                       -- NULL = timer still running
  note:        TEXT
  archived_at: TEXT
  created_at:  TEXT NOT NULL
```

### CaptureItems (Inbox)

```
capture_items
  id:          INTEGER PRIMARY KEY
  slug:        TEXT NOT NULL UNIQUE       -- e.g. "capture-20260331-143000"
  body:        TEXT NOT NULL
  processed:   INTEGER NOT NULL DEFAULT 0 -- boolean
  created_at:  TEXT NOT NULL
```

### Reminders

```
reminders
  id:          INTEGER PRIMARY KEY
  slug:        TEXT NOT NULL UNIQUE       -- e.g. "payments-reminder-deploy-friday"
  project_id:  INTEGER NOT NULL REFERENCES projects(id) ON DELETE RESTRICT
  task_id:     INTEGER REFERENCES tasks(id) ON DELETE SET NULL  -- optional
  remind_at:   TEXT NOT NULL              -- ISO 8601
  message:     TEXT
  fired:       INTEGER NOT NULL DEFAULT 0
  archived_at: TEXT
  created_at:  TEXT NOT NULL
```

---

## CLI Command Surface

All entities are addressed by their **slug** — never by a numeric ID.
Slugs tab-complete in the shell (see Tab Completion section below).
If `--project` is omitted when creating an item, it is assigned to
`quick-capture` automatically.

```
scribe
├── project
│   ├── add     <slug> --name <name> [--desc <text>]
│   ├── list    [--status active|paused|completed] [--archived]
│   ├── show    <slug>
│   ├── edit    <slug> [--slug <new>] [--name <new>] [--desc <text>] [--status <s>]
│   ├── archive <slug>
│   ├── restore <slug>
│   └── delete  <slug>    (prompts to archive items; blocked if reserved)
├── task
│   ├── add     <title> [--project <slug>] [--priority low|medium|high|urgent] [--due <date>]
│   │           (auto-generates slug, e.g. payments-task-fix-login)
│   ├── list    [--project <slug>] [--status <s>] [--priority <p>] [--due-before <date>] [--archived]
│   ├── show    <slug>
│   ├── edit    <slug> [--title <t>] [--status <s>] [--priority <p>] [--due <date>]
│   ├── move    <slug> --project <slug>    (recategorise to a different project)
│   ├── done    <slug>
│   ├── archive <slug>
│   ├── restore <slug>
│   └── delete  <slug>
├── todo
│   ├── add     <title> [--project <slug>]
│   │           (auto-generates slug, e.g. payments-todo-review-docs)
│   ├── list    [--project <slug>] [--all] [--archived]
│   ├── show    <slug>
│   ├── move    <slug> --project <slug>
│   ├── done    <slug>
│   ├── archive <slug>
│   ├── restore <slug>
│   └── delete  <slug>
├── track
│   ├── start   [--task <slug>] [--project <slug>] [--note <text>]
│   │           (auto-generates entry slug, e.g. payments-entry-20260331-143000)
│   ├── stop
│   ├── status
│   └── report  [--today] [--week] [--project <slug>]
├── capture     <text>    (auto-generates capture slug; no project needed)
├── inbox
│   ├── list
│   └── process <slug>    (interactive: assign project, convert to task/todo/reminder)
└── reminder
    ├── add     --project <slug> --at <datetime> [--task <slug>] [--message <text>]
    │           (auto-generates slug, e.g. payments-reminder-deploy-friday)
    ├── list    [--project <slug>] [--archived]
    ├── show    <slug>
    ├── archive <slug>
    ├── restore <slug>
    └── delete  <slug>
```

Additionally, running `scribe` with no arguments opens the TUI.
Every subcommand supports `--output json` for machine-readable output.

---

## Tab Completion

Shell tab completion covers two layers:

**Static completion** (provided by `clap_complete`):
- Subcommand names
- Flag names (`--project`, `--status`, `--priority`, etc.)
- Enum flag values (`--status <TAB>` → `todo  in_progress  done  cancelled`)

**Dynamic completion** (provided by a hidden `scribe __complete <entity>` subcommand):
- Project slugs
- Task slugs (with title hint: `payments-task-fix-login\tFix the login bug`)
- Todo, reminder, capture item slugs (same format)
- Slugs are fetched live from the local DB

The hidden subcommand prints `<slug>\t<display-hint>` pairs to stdout, one per
line. The shell completion script calls it and renders the hint as a description
alongside the completion candidate (supported natively in zsh and fish; bash
shows slugs only).

**Installation** — users run `scribe completions <shell>` to print the
completion script, then source it from their shell config:

```sh
# zsh
scribe completions zsh > ~/.zfunc/_scribe

# bash
scribe completions bash >> ~/.bash_completion

# fish
scribe completions fish > ~/.config/fish/completions/scribe.fish
```

Supported shells: `bash`, `zsh`, `fish`, `elvish`, `powershell`.

---

## TUI Layout

```
┌────────────────────────────────────────────────────────┐
│  Scribe  [D]ashboard [P]rojects [T]asks [O]Todos        │
│          T[R]acker   [I]nbox    [M]Reminders            │
├────────────────────────────────────────────────────────┤
│                                                        │
│         (active view rendered here)                    │
│                                                        │
├────────────────────────────────────────────────────────┤
│  ⏱ Active timer: task #42 "Write tests" — 0h 23m       │
│  ? Help  q Quit  Tab Next pane  hjkl Navigate          │
└────────────────────────────────────────────────────────┘
```

Key bindings are consistent across all views:
- `j` / `k` — move selection down/up
- `Enter` — open/expand item
- `n` — new item
- `e` — edit selected item
- `d` — delete selected item (confirmation required)
- `Space` — toggle done (todos) / start timer (tasks)
- `/` — fuzzy filter
- `Esc` — back / close modal
- `?` — toggle help overlay
- `q` — quit

---

## Error Handling Strategy

This project follows **M-ERRORS-CANONICAL-STRUCTS** for domain/ops layers and
**M-APP-ERROR** at the application boundary.

- Domain and ops errors are situation-specific `struct`s (e.g. `ProjectNotFound`, `TimerAlreadyRunning`).
- Each error struct contains an internal `ErrorKind` enum when operations are mixed; kind is
  exposed via `is_xxx()` methods, never the enum directly.
- Each error implements `Display` (summary sentence + backtrace), `std::error::Error`, and `Debug`.
- SQLite errors are mapped to domain error structs — no raw `rusqlite::Error` leaks through
  public API boundaries (M-DONT-LEAK-TYPES).
- CLI handlers use `anyhow::Result` (M-APP-ERROR). Errors are printed to stderr; exit codes: `0` success, `1` user error, `2` internal error.
- The TUI never panics; errors are displayed in the status bar.

---

## Naming Conventions (M-CONCISE-NAMES)

Weasel words (`Service`, `Manager`, `Factory`) are banned.

| Layer | Naming pattern | Example |
|-------|---------------|---------|
| Domain entity | `PascalCase` noun | `Project`, `Task`, `TimeEntry` |
| Repository trait | Plural noun | `Projects`, `Tasks`, `TimeEntries` |
| SQLite implementation | `Sqlite` prefix + plural noun | `SqliteProjects`, `SqliteTasks` |
| Operations module | Plural noun | `ops::projects`, `ops::tracker` |
| Newtype ID wrappers | `<Entity>Id` | `ProjectId`, `TaskId` |

---

## Static Verification (M-STATIC-VERIFICATION)

`Cargo.toml` must include these lint tables:

```toml
[lints.rust]
ambiguous_negative_literals       = "warn"
missing_debug_implementations     = "warn"
redundant_imports                 = "warn"
redundant_lifetimes               = "warn"
trivial_numeric_casts             = "warn"
unsafe_op_in_unsafe_fn            = "warn"
unused_lifetimes                  = "warn"

[lints.clippy]
cargo                             = { level = "warn", priority = -1 }
complexity                        = { level = "warn", priority = -1 }
correctness                       = { level = "warn", priority = -1 }
pedantic                          = { level = "warn", priority = -1 }
perf                              = { level = "warn", priority = -1 }
style                             = { level = "warn", priority = -1 }
suspicious                        = { level = "warn", priority = -1 }
allow_attributes_without_reason   = "warn"
as_pointer_underscore             = "warn"
assertions_on_result_states       = "warn"
clone_on_ref_ptr                  = "warn"
deref_by_slicing                  = "warn"
disallowed_script_idents          = "warn"
empty_drop                        = "warn"
empty_enum_variants_with_brackets = "warn"
empty_structs_with_brackets       = "warn"
fn_to_numeric_cast_any            = "warn"
if_then_some_else_none            = "warn"
map_err_ignore                    = "warn"
redundant_type_annotations        = "warn"
renamed_function_params           = "warn"
semicolon_outside_block           = "warn"
string_to_string                  = "warn"
undocumented_unsafe_blocks        = "warn"
unnecessary_safety_comment        = "warn"
unnecessary_safety_doc            = "warn"
unneeded_field_pattern            = "warn"
unused_result_ok                  = "warn"
literal_string_with_formatting_args = "allow"
```

Required tools: `cargo fmt`, `cargo clippy`, `cargo audit`, `cargo-udeps`.
Zero warnings policy: `cargo clippy -- -D warnings` must pass on every commit.

---

## Documentation Standards (M-CANONICAL-DOCS, M-MODULE-DOCS)

Every public item must have:

```rust
/// Summary sentence under 15 words.
///
/// Extended documentation.
///
/// # Examples
/// ...
///
/// # Errors      (when returning Result)
/// ...
///
/// # Panics      (when the function can panic)
/// ...
///
/// # Safety      (when unsafe)
/// ...
```

Every public module must have a `//!` module-level doc comment.

No parameter tables — parameters are described inline in prose.

---

## Testing Strategy

- **Unit tests** live alongside source in `#[cfg(test)]` modules.
- **Ops tests** use in-memory SQLite (`:memory:`).
- **CLI integration tests** live in `tests/` and use `assert_cmd` + `tempfile`.
- Use `rstest` for parametric tests.
- Test names: `test_<function>_<scenario>`.
- Mocking via trait substitution — no mocking crates.
- Target ≥ 80% line coverage on ops and domain layers.
- I/O and system calls are mockable (M-MOCKABLE-SYSCALLS); mocking support is
  feature-gated under `test-util` (M-TEST-UTIL).

---

## Configuration File

Located at `$XDG_CONFIG_HOME/scribe/config.toml` (defaults to `~/.config/scribe/config.toml`).

```toml
[data]
db_path = ""  # defaults to $XDG_DATA_HOME/scribe/scribe.db

[notifications]
enabled = true

[display]
date_format = "%Y-%m-%d"
time_format = "%H:%M"
```

---

## Phased Implementation Roadmap

### Phase 1 — Foundation (MVP)

Goal: compiling binary with working persistence and basic CLI.

1. Set up `Cargo.toml` with all dependencies and required lint tables
2. Implement `config.rs` — XDG data directory, DB path
3. Implement `db/` — connection, migrations, schema (including slug columns)
4. Implement `domain/slug.rs` — slug generation and collision handling
5. Implement `domain/` structs, newtype IDs, and repository traits
6. Implement `store/` — SQLite repository implementations
7. Implement `ops/` for projects and tasks
8. Implement `cli/` for `project` and `task` subcommands
9. Wire `main.rs` with `mimalloc` allocator
10. Write integration tests

### Phase 2 — Complete CLI

Goal: all CLI subcommands working.

11. Todos CLI + ops
12. Time tracking CLI + ops
13. Quick capture CLI + ops
14. Reminders CLI + ops

### Phase 3 — TUI Core

Goal: navigable TUI with read-only views.

15. TUI event loop + app state machine
16. Dashboard view
17. Projects view
18. Tasks view

### Phase 4 — TUI Full Feature

Goal: TUI on par with CLI.

19. Todos view
20. Tracker view
21. Inbox view
22. Reminders view
23. Inline forms (create/edit) within TUI

### Phase 5 — Polish

24. Shell completions — static (`clap_complete`) + dynamic (`scribe __complete`)
25. Desktop notifications (feature-gated)
26. Export to JSON/CSV
27. `man` page generation
28. Performance profiling + optimisation
