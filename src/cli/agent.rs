//! Agent skill-file installer — `scribe agent install`.
//!
//! Detects which AI coding agent tool directories are present on the machine
//! and writes a `scribe.md` skill file to each one.  Reports which agents
//! were found and installed and which were skipped.
//!
//! Agents supported:
//!
//! | Agent | Directory |
//! |---|---|
//! | Claude Code / OpenCode | `~/.claude/skills/` |
//! | Cursor | `~/.cursor/rules/` |
//! | Codex | `~/.codex/` |
//! | Windsurf | `~/.windsurf/rules/` |

use std::path::Path;

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::cli::project::OutputFormat;
use crate::cli::service::home_dir;

// ── Clap argument structs ──────────────────────────────────────────────────

/// Arguments for the `agent` subcommand group.
#[derive(Debug, clap::Subcommand)]
pub enum AgentCommand {
    /// Install the Scribe skill file to all detected agent directories.
    Install(AgentInstallArgs),
}

/// Arguments for `scribe agent install`.
#[derive(Debug, Args)]
pub struct AgentInstallArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
}

// ── Agent registry ─────────────────────────────────────────────────────────

/// A single agent target — its display name, skill directory, and output filename.
#[derive(Debug, Clone)]
struct AgentTarget {
    /// Human-readable display label, e.g. `"Claude Code / OpenCode"`.
    name: &'static str,
    /// Relative-to-home path of the skill directory.
    dir: &'static str,
    /// Filename to write inside the skill directory.
    filename: &'static str,
}

/// All known agent targets checked at runtime.
///
/// Order determines display order in the output.
// DOCUMENTED-MAGIC: Five agents are listed here; paths are checked at
// runtime and skipped silently when absent, so adding entries never breaks
// existing setups.
const AGENT_TARGETS: &[AgentTarget] = &[
    AgentTarget {
        name: "Claude Code / OpenCode",
        dir: ".claude/skills",
        filename: "scribe.md",
    },
    AgentTarget {
        name: "Cursor",
        dir: ".cursor/rules",
        filename: "scribe.md",
    },
    AgentTarget {
        name: "Codex",
        dir: ".codex",
        filename: "scribe.md",
    },
    AgentTarget {
        name: "Windsurf",
        dir: ".windsurf/rules",
        filename: "scribe.md",
    },
];

// ── Result types ───────────────────────────────────────────────────────────

/// Outcome for a single agent during `scribe agent install`.
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentInstallResult {
    /// Display name of the agent.
    pub agent: String,
    /// `"installed"` or `"skipped"`.
    pub status: String,
    /// Absolute path written (only present when `status == "installed"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Human-readable reason for the skip (only present when `status == "skipped"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── Skill file content ─────────────────────────────────────────────────────

/// Returns the Markdown content written to each agent's skill file.
///
/// The content teaches the agent what Scribe is, when to use it, all CLI
/// commands, the JSON output flag, MCP integration notes, workflow examples,
/// and key behaviours.
// The skill file is intentionally comprehensive; it cannot be split without
// losing coherence, so the line-count lint is suppressed here (M-LINT-OVERRIDE-EXPECT).
#[expect(
    clippy::too_many_lines,
    reason = "skill file content must be comprehensive and self-contained; splitting would lose coherence"
)]
fn skill_content() -> String {
    r#"# Scribe — AI Agent Skill

## What is Scribe?

Scribe is an offline-first personal productivity CLI/TUI written in Rust. It
stores projects, tasks, todos, time entries, a quick-capture inbox, and
reminders in a single local SQLite file (`~/.local/share/scribe/scribe.db`).
Every operation is available as a scriptable CLI subcommand and also through
a keyboard-driven full-screen TUI (`scribe` with no arguments).

## When to use Scribe

Suggest using Scribe tools whenever the user mentions:

- Tasks, work items, action items, or things to do.
- Projects, milestones, or grouping work.
- Time tracking, logging hours, or recording effort.
- Todos, checklists, or lightweight reminders.
- Reminders, scheduled nudges, or follow-ups.
- Quick capture, brain-dump, or inbox-processing.
- Any personal productivity or work-management need.

## Core concepts

### Projects

A project is the top-level container. Every task, todo, time entry, and
reminder must belong to exactly one project.

### The `quick-capture` project

A reserved project (`slug = "quick-capture"`) is seeded automatically on first
run. It cannot be archived or deleted. Items created without `--project` land
here by default.

### Slugs (user-facing identifiers)

| Entity | Slug format | Example |
|---|---|---|
| Project | user-defined kebab-case | `payments` |
| Task | `{project}-task-{title-slug}` | `payments-task-fix-login` |
| Todo | `{project}-todo-{title-slug}` | `payments-todo-review-docs` |
| Reminder | `{project}-reminder-{msg-slug}` | `payments-reminder-deploy-friday` |
| TimeEntry | `{project}-entry-{YYYYMMDD}-{HHmmss}` | `payments-entry-20260331-143000` |
| CaptureItem | `capture-{YYYYMMDD}-{HHmmss}` | `capture-20260331-143000` |

### Archive model

There are **no hard deletes** for user data. Archive items first; then delete
them from the archive. Archiving a project cascades to all its linked items.
Use `--archived` or `restore` to access archived items.

---

## CLI command reference

Every subcommand accepts `--output json` for machine-readable structured output.
**Always use `--output json` when scripting.**

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
├── reminder
│   ├── add     --project <slug> --at <datetime> [--task <slug>] [--message <text>] [--output json]
│   ├── list    [--project <slug>] [--archived] [--output json]
│   ├── show    <slug> [--output json]
│   ├── archive <slug> [--output json]
│   ├── restore <slug> [--output json]
│   └── delete  <slug> [--output json]
└── agent
    └── install [--output json]
```

Running `scribe` with no subcommand opens the TUI.

### `--at` datetime formats for reminders

| Input | Interpretation |
|---|---|
| `2026-04-01T14:00:00` | ISO 8601, local time |
| `2026-04-01 14:00` | Space-separated, local time |
| `2026-04-01` | Date only, midnight local |
| `tomorrow 09:00` | Next calendar day |
| `friday 17:00` | Coming Friday at 17:00 |
| `friday` | Coming Friday at 09:00 |

---

## JSON output

**Always use `--output json` when scripting.** Every subcommand returns
structured JSON:

```sh
# List active projects as JSON
scribe project list --output json

# Create a task and capture the new slug
SLUG=$(scribe task add "Fix login bug" --project payments --output json | jq -r '.slug')
```

---

## MCP server integration

If the user has the MCP server configured (requires building with `--features mcp`),
**prefer MCP tools over CLI subcommands** for read/write operations — they are
faster (no subprocess) and return structured data directly.

MCP server startup message (appears on stderr):
```
Scribe MCP server running on stdio. Connect your agent to this process.
```

### MCP tools available

**Projects:** `project_list`, `project_create`, `project_archive`, `project_restore`

**Tasks:** `task_list`, `task_create`, `task_done`, `task_archive`

**Todos:** `todo_list`, `todo_create`, `todo_done`, `todo_archive`

**Time tracking:** `timer_start`, `timer_stop`, `timer_status`, `track_report`

**Capture / inbox:** `capture`, `inbox_list`, `inbox_process`

**Reminders:** `reminder_list`, `reminder_create`, `reminder_archive`

### MCP resources available

| URI | Contents |
|---|---|
| `scribe://projects` | All active projects (JSON) |
| `scribe://tasks/active` | All active non-archived tasks (JSON) |
| `scribe://todos/active` | All active non-done todos (JSON) |
| `scribe://timer/active` | Running timer or `null` (JSON) |
| `scribe://inbox` | Unprocessed capture items (JSON) |
| `scribe://reminders/pending` | Active unfired reminders (JSON) |

---

## Workflow examples

### 1. Add a task

```sh
scribe task add "Fix the login redirect bug" --project payments --priority high
```

### 2. Start a timer

```sh
scribe track start --task payments-task-fix-the-login-redirect-bug
# or simply:
scribe track start --project payments --note "Debugging auth middleware"
```

### 3. Quick capture

```sh
scribe capture "Look into Redis caching for the payment API"
# Captured: capture-20260331-143512
# Process later: scribe inbox process capture-20260331-143512
```

### 4. Generate a time report

```sh
scribe track report --today --project payments
# Machine-readable:
scribe track report --week --output json | jq '.[] | {slug, duration_secs}'
```

### 5. Mark a task done

```sh
scribe task done payments-task-fix-the-login-redirect-bug
```

---

## Key behaviours

- **Default project**: items created without `--project` go to `quick-capture`.
- **No hard deletes**: archive first (`task archive <slug>`), then delete.
- **Slugs are globally unique** across all entities; they are the user-facing
  identifier used everywhere in the CLI.
- **Archiving a project cascades** to all its tasks, todos, time entries, and
  reminders. Restoring the project does **not** auto-restore those items.
- **One timer at a time**: `scribe track start` fails if a timer is already
  running. Stop it first with `scribe track stop`.
- **Tab completion**: run `scribe completions zsh` (or `bash`/`fish`) to
  install shell completions including live slug lookup from the database.
"#
    .to_owned()
}

// ── MCP config snippets ────────────────────────────────────────────────────

/// Returns the MCP config snippet for Claude Code (paste to `~/.claude/settings.json`).
fn claude_mcp_snippet() -> &'static str {
    r#"{
  "mcpServers": {
    "scribe": {
      "command": "scribe",
      "args": ["mcp"]
    }
  }
}"#
}

/// Returns the MCP config snippet for `OpenCode` (paste to `opencode.json`).
fn opencode_mcp_snippet() -> &'static str {
    r#"{
  "mcp": {
    "scribe": {
      "type": "local",
      "command": ["scribe", "mcp"],
      "enabled": true
    }
  }
}"#
}

// ── Runner ─────────────────────────────────────────────────────────────────

/// Executes `scribe agent install` and prints results to stdout.
///
/// Iterates over [`AGENT_TARGETS`], checks whether each skill directory
/// exists, writes the skill file when it does, and prints a summary line.
/// With `--output json` emits a JSON array instead.
///
/// `config` is mutated to record that agent install completed so that
/// `scribe setup` can reflect the state.
///
/// # Errors
///
/// Returns an error if the home directory cannot be found or if a file-system
/// write fails.
pub fn run(args: &AgentInstallArgs, config: &mut crate::config::Config) -> anyhow::Result<()> {
    let home = home_dir()?;
    let content = skill_content();

    let mut results: Vec<AgentInstallResult> = Vec::new();

    for target in AGENT_TARGETS {
        let dir_path = home.join(target.dir);
        let result = try_install(target, &dir_path, &content);
        results.push(result);
    }

    match args.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        OutputFormat::Text => {
            print_text_results(&results);
            print_mcp_snippets();
        }
    }

    // Mark agent as installed in config (at least one install succeeded).
    let any_installed = results.iter().any(|r| r.status == "installed");
    if any_installed {
        config.setup.agent_installed = true;
        // Best-effort save — failure is non-fatal.
        let _ = config.save();
    }

    Ok(())
}

/// Attempts to install the skill file for a single target.
///
/// Returns an [`AgentInstallResult`] describing whether the installation
/// succeeded or was skipped.
fn try_install(target: &AgentTarget, dir_path: &Path, content: &str) -> AgentInstallResult {
    if !dir_path.exists() {
        return AgentInstallResult {
            agent: target.name.to_owned(),
            status: "skipped".to_owned(),
            path: None,
            reason: Some("directory not found".to_owned()),
        };
    }

    let file_path = dir_path.join(target.filename);

    match std::fs::write(&file_path, content) {
        Ok(()) => AgentInstallResult {
            agent: target.name.to_owned(),
            status: "installed".to_owned(),
            path: Some(file_path.to_string_lossy().into_owned()),
            reason: None,
        },
        Err(err) => AgentInstallResult {
            agent: target.name.to_owned(),
            status: "skipped".to_owned(),
            path: None,
            reason: Some(format!("write failed: {err}")),
        },
    }
}

/// Prints the text-mode install summary to stdout.
fn print_text_results(results: &[AgentInstallResult]) {
    // DOCUMENTED-MAGIC: Column width 44 chosen so the path column is
    // consistently indented for all current agent names.
    const LABEL_WIDTH: usize = 44;
    for r in results {
        if r.status.as_str() == "installed" {
            let path = r.path.as_deref().unwrap_or("");
            println!(
                "Installed skill for {:<width$} ({})",
                r.agent,
                path,
                width = LABEL_WIDTH
            );
        } else {
            let reason = r.reason.as_deref().unwrap_or("unknown");
            println!("Skipped: {} ({})", r.agent, reason);
        }
    }
}

/// Prints MCP configuration snippets to stdout after the install summary.
fn print_mcp_snippets() {
    println!();
    println!("--- MCP server configuration snippets (paste into your agent config) ---");
    println!();
    println!("Claude Code — add to ~/.claude/settings.json:");
    println!("{}", claude_mcp_snippet());
    println!();
    println!("OpenCode — add to your opencode.json:");
    println!("{}", opencode_mcp_snippet());
    println!();
    println!(
        "Note: the MCP server requires building with the `mcp` feature:\n\
         \n  cargo install --path . --features mcp\n\
         \nDo NOT run `scribe mcp` in a terminal — stdout is the MCP wire protocol."
    );
}
