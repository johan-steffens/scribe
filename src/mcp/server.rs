//! MCP server implementation — tools and resources for Scribe data.
//!
//! [`ScribeMcpServer`] implements [`rmcp::ServerHandler`] and exposes all
//! Scribe entities as MCP tools (callable operations) and MCP resources
//! (readable data URIs).
//!
//! # Tool groups
//!
//! | Group | Tools |
//! |---|---|
//! | Projects | `project_list`, `project_create`, `project_archive`, `project_restore` |
//! | Tasks | `task_list`, `task_create`, `task_done`, `task_archive` |
//! | Todos | `todo_list`, `todo_create`, `todo_done`, `todo_archive` |
//! | Time tracking | `timer_start`, `timer_stop`, `timer_status`, `track_report` |
//! | Capture / inbox | `capture`, `inbox_list`, `inbox_process` |
//! | Reminders | `reminder_list`, `reminder_create`, `reminder_archive` |
//!
//! # Resource URIs
//!
//! | URI | Contents |
//! |---|---|
//! | `scribe://projects` | Active projects (JSON array) |
//! | `scribe://tasks/active` | Active non-archived tasks (JSON array) |
//! | `scribe://todos/active` | Active non-done todos (JSON array) |
//! | `scribe://timer/active` | Running timer JSON object, or `null` |
//! | `scribe://inbox` | Unprocessed capture items (JSON array) |
//! | `scribe://reminders/pending` | Active unfired reminders (JSON array) |
//!
//! # Error handling
//!
//! All tool methods return a JSON text result.  On error the result is a
//! JSON object `{"error": "<message>"}` — the server never panics.

#![cfg(feature = "mcp")]

use std::sync::{Arc, Mutex};

use chrono::Datelike as _;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        AnnotateAble as _, ListResourcesResult, ReadResourceRequestParams, ReadResourceResult,
        ResourceContents, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ops::{InboxOps, ProjectOps, ReminderOps, TaskOps, TodoOps, TrackerOps};

// ── Parameter structs ───────────────────────────────────────────────────────

/// Parameters for `project_create`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectCreateParams {
    /// Unique kebab-case project slug.
    pub slug: String,
    /// Human-readable project name.
    pub name: String,
    /// Optional free-text description.
    pub description: Option<String>,
}

/// Parameters for `project_archive` / `project_restore`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectSlugParam {
    /// Project slug.
    pub slug: String,
}

/// Parameters for `task_list`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskListParams {
    /// Filter to a specific project slug.
    pub project_slug: Option<String>,
    /// Filter by status: `"todo"`, `"in_progress"`, `"done"`, `"cancelled"`.
    pub status: Option<String>,
}

/// Parameters for `task_create`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskCreateParams {
    /// Task title.
    pub title: String,
    /// Project slug; defaults to `quick-capture` if omitted.
    pub project_slug: Option<String>,
    /// Priority: `"low"`, `"medium"` (default), `"high"`, `"urgent"`.
    pub priority: Option<String>,
    /// Due date as `YYYY-MM-DD`.
    pub due_date: Option<String>,
}

/// Parameters for `task_done` / `task_archive`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskSlugParam {
    /// Task slug.
    pub slug: String,
}

/// Parameters for `todo_list`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TodoListParams {
    /// Filter to a specific project slug.
    pub project_slug: Option<String>,
}

/// Parameters for `todo_create`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TodoCreateParams {
    /// Todo title.
    pub title: String,
    /// Project slug; defaults to `quick-capture` if omitted.
    pub project_slug: Option<String>,
}

/// Parameters for `todo_done` / `todo_archive`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TodoSlugParam {
    /// Todo slug.
    pub slug: String,
}

/// Parameters for `timer_start`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TimerStartParams {
    /// Project slug; defaults to `quick-capture`.
    pub project_slug: Option<String>,
    /// Optional linked task slug.
    pub task_slug: Option<String>,
    /// Optional free-text note.
    pub note: Option<String>,
}

/// Parameters for `track_report`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrackReportParams {
    /// Filter by project slug.
    pub project_slug: Option<String>,
    /// Time window: `"today"`, `"week"`, or omit for all-time.
    pub period: Option<String>,
}

/// Parameters for `capture`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CaptureParams {
    /// Text body to capture.
    pub body: String,
}

/// Parameters for `inbox_process`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InboxProcessParams {
    /// Capture item slug.
    pub slug: String,
    /// Action: `"task"`, `"todo"`, `"assign"`, or `"discard"`.
    pub action: String,
    /// Destination project slug (required for `task`, `todo`, `assign`).
    pub project_slug: Option<String>,
    /// Optional title override.
    pub title: Option<String>,
}

/// Parameters for `reminder_list`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReminderListParams {
    /// Filter to a specific project slug.
    pub project_slug: Option<String>,
}

/// Parameters for `reminder_create`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReminderCreateParams {
    /// Owning project slug.
    pub project_slug: String,
    /// Fire datetime — same flexible formats as the CLI `--at` flag.
    pub at: String,
    /// Optional linked task slug.
    pub task_slug: Option<String>,
    /// Optional message text.
    pub message: Option<String>,
    /// When `true`, the notification blocks until the user dismisses it.
    ///
    /// On macOS uses `display alert` (modal). Defaults to `false` (banner).
    pub persistent: Option<bool>,
}

/// Parameters for `reminder_archive`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReminderSlugParam {
    /// Reminder slug.
    pub slug: String,
}

// ── Server struct ───────────────────────────────────────────────────────────

/// The Scribe MCP server — implements [`ServerHandler`] via `#[tool_router]`.
///
/// Holds all ops structs and a reference to the shared DB connection.
/// Each tool method acquires the connection lock via the ops layer.
pub struct ScribeMcpServer {
    projects: ProjectOps,
    tasks: TaskOps,
    todos: TodoOps,
    tracker: TrackerOps,
    inbox: InboxOps,
    reminders: ReminderOps,
    /// The tool router generated by `#[tool_router]`.
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for ScribeMcpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScribeMcpServer").finish_non_exhaustive()
    }
}

impl ScribeMcpServer {
    /// Creates a new [`ScribeMcpServer`] using the given database connection.
    pub fn new(conn: &Arc<Mutex<Connection>>) -> Self {
        Self {
            projects: ProjectOps::new(conn),
            tasks: TaskOps::new(Arc::clone(conn)),
            todos: TodoOps::new(Arc::clone(conn)),
            tracker: TrackerOps::new(Arc::clone(conn)),
            inbox: InboxOps::new(conn),
            reminders: ReminderOps::new(Arc::clone(conn)),
            tool_router: Self::tool_router(),
        }
    }
}

// ── Helper serialisation ───────────────────────────────────────────────────

/// Serialises a value to a pretty JSON string, falling back to an error message.
fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| format!("{{\"error\":\"serialise: {e}\"}}"))
}

/// Serialises an error to a JSON `{"error": "..."}` string.
fn err_json(msg: impl std::fmt::Display) -> String {
    serde_json::json!({"error": msg.to_string()}).to_string()
}

// ── Tool implementations ────────────────────────────────────────────────────

#[tool_router]
impl ScribeMcpServer {
    // ── Projects ────────────────────────────────────────────────────────────

    /// Lists all active (non-archived) projects.
    #[tool(description = "List all active Scribe projects.")]
    async fn project_list(&self) -> String {
        match self.projects.list_projects(None, false) {
            Ok(projects) => to_json(&projects),
            Err(e) => err_json(e),
        }
    }

    /// Creates a new project.
    #[tool(
        description = "Create a new Scribe project with a slug, name, and optional description."
    )]
    async fn project_create(&self, Parameters(p): Parameters<ProjectCreateParams>) -> String {
        use crate::domain::{NewProject, ProjectStatus};
        let new_project = NewProject {
            slug: p.slug,
            name: p.name,
            description: p.description,
            status: ProjectStatus::Active,
        };
        match self.projects.create_project(new_project) {
            Ok(project) => to_json(&project),
            Err(e) => err_json(e),
        }
    }

    /// Archives a project and all its linked items.
    #[tool(
        description = "Archive a Scribe project and all its tasks, todos, entries, and reminders."
    )]
    async fn project_archive(&self, Parameters(p): Parameters<ProjectSlugParam>) -> String {
        match self.projects.archive_project(&p.slug) {
            Ok(project) => to_json(&project),
            Err(e) => err_json(e),
        }
    }

    /// Restores an archived project.
    #[tool(description = "Restore an archived Scribe project.")]
    async fn project_restore(&self, Parameters(p): Parameters<ProjectSlugParam>) -> String {
        match self.projects.restore_project(&p.slug) {
            Ok(project) => to_json(&project),
            Err(e) => err_json(e),
        }
    }

    // ── Tasks ───────────────────────────────────────────────────────────────

    /// Lists active tasks with optional project and status filters.
    #[tool(
        description = "List active Scribe tasks, optionally filtered by project_slug and/or status."
    )]
    async fn task_list(&self, Parameters(p): Parameters<TaskListParams>) -> String {
        use crate::domain::{ProjectId, TaskStatus};
        use std::str::FromStr as _;

        let status = match p.status.as_deref() {
            Some(s) => match TaskStatus::from_str(s) {
                Ok(st) => Some(st),
                Err(e) => return err_json(e),
            },
            None => None,
        };

        let project_id = if let Some(ref slug) = p.project_slug {
            match self.projects.get_project(slug) {
                Ok(Some(pr)) => Some(pr.id),
                Ok(None) => return err_json(format!("project '{slug}' not found")),
                Err(e) => return err_json(e),
            }
        } else {
            None::<ProjectId>
        };

        match self.tasks.list_tasks(project_id, status, None, false) {
            Ok(tasks) => to_json(&tasks),
            Err(e) => err_json(e),
        }
    }

    /// Creates a new task.
    #[tool(
        description = "Create a new Scribe task. Defaults to quick-capture project and medium priority."
    )]
    async fn task_create(&self, Parameters(p): Parameters<TaskCreateParams>) -> String {
        use crate::domain::{TaskPriority, TaskStatus};
        use crate::ops::tasks::CreateTask;
        use chrono::NaiveDate;
        use std::str::FromStr as _;

        let project_slug = p.project_slug.unwrap_or_else(|| "quick-capture".to_owned());

        let project = match self.projects.get_project(&project_slug) {
            Ok(Some(pr)) => pr,
            Ok(None) => return err_json(format!("project '{project_slug}' not found")),
            Err(e) => return err_json(e),
        };

        let priority = match p.priority.as_deref() {
            Some(s) => match TaskPriority::from_str(s) {
                Ok(pr) => pr,
                Err(e) => return err_json(e),
            },
            None => TaskPriority::Medium,
        };

        let due_date = if let Some(ref d) = p.due_date {
            match NaiveDate::from_str(d) {
                Ok(date) => Some(date),
                Err(e) => return err_json(format!("invalid due date '{d}': {e}")),
            }
        } else {
            None
        };

        let params = CreateTask {
            project_slug: project.slug.clone(),
            project_id: project.id,
            title: p.title,
            description: None,
            status: TaskStatus::Todo,
            priority,
            due_date,
        };

        match self.tasks.create_task(params) {
            Ok(task) => to_json(&task),
            Err(e) => err_json(e),
        }
    }

    /// Marks a task as done.
    #[tool(description = "Mark a Scribe task as done.")]
    async fn task_done(&self, Parameters(p): Parameters<TaskSlugParam>) -> String {
        use crate::domain::{TaskPatch, TaskStatus};
        match self.tasks.update_task(
            &p.slug,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        ) {
            Ok(task) => to_json(&task),
            Err(e) => err_json(e),
        }
    }

    /// Archives a task.
    #[tool(description = "Archive a Scribe task.")]
    async fn task_archive(&self, Parameters(p): Parameters<TaskSlugParam>) -> String {
        match self.tasks.archive_task(&p.slug) {
            Ok(task) => to_json(&task),
            Err(e) => err_json(e),
        }
    }

    // ── Todos ───────────────────────────────────────────────────────────────

    /// Lists active todos with optional project filter.
    #[tool(description = "List active Scribe todos, optionally filtered by project_slug.")]
    async fn todo_list(&self, Parameters(p): Parameters<TodoListParams>) -> String {
        use crate::domain::ProjectId;

        let project_id = if let Some(ref slug) = p.project_slug {
            match self.projects.get_project(slug) {
                Ok(Some(pr)) => Some(pr.id),
                Ok(None) => return err_json(format!("project '{slug}' not found")),
                Err(e) => return err_json(e),
            }
        } else {
            None::<ProjectId>
        };

        match self.todos.list(project_id, false, false) {
            Ok(todos) => to_json(&todos),
            Err(e) => err_json(e),
        }
    }

    /// Creates a new todo.
    #[tool(description = "Create a new Scribe todo. Defaults to quick-capture project.")]
    async fn todo_create(&self, Parameters(p): Parameters<TodoCreateParams>) -> String {
        let project_slug = p.project_slug.unwrap_or_else(|| "quick-capture".to_owned());
        match self.todos.create(&project_slug, &p.title) {
            Ok(todo) => to_json(&todo),
            Err(e) => err_json(e),
        }
    }

    /// Marks a todo as done.
    #[tool(description = "Mark a Scribe todo as done.")]
    async fn todo_done(&self, Parameters(p): Parameters<TodoSlugParam>) -> String {
        match self.todos.mark_done(&p.slug) {
            Ok(todo) => to_json(&todo),
            Err(e) => err_json(e),
        }
    }

    /// Archives a todo.
    #[tool(description = "Archive a Scribe todo.")]
    async fn todo_archive(&self, Parameters(p): Parameters<TodoSlugParam>) -> String {
        match self.todos.archive(&p.slug) {
            Ok(todo) => to_json(&todo),
            Err(e) => err_json(e),
        }
    }

    // ── Timer / time tracking ───────────────────────────────────────────────

    /// Starts a new timer.
    #[tool(description = "Start a Scribe timer, optionally linked to a project and/or task.")]
    async fn timer_start(&self, Parameters(p): Parameters<TimerStartParams>) -> String {
        use crate::ops::tracker::StartTimer;

        let project_slug = p.project_slug.unwrap_or_else(|| "quick-capture".to_owned());

        let (slug, project_id) = match self.tracker.resolve_project(&project_slug) {
            Ok(pair) => pair,
            Err(e) => return err_json(e),
        };

        let task_id = if let Some(ref ts) = p.task_slug {
            match self.tasks.get_task(ts) {
                Ok(Some(t)) => Some(t.id),
                Ok(None) => return err_json(format!("task '{ts}' not found")),
                Err(e) => return err_json(e),
            }
        } else {
            None
        };

        match self.tracker.start_timer(StartTimer {
            project_slug: slug,
            project_id,
            task_id,
            note: p.note,
        }) {
            Ok(entry) => to_json(&entry),
            Err(e) => err_json(e),
        }
    }

    /// Stops the currently running timer.
    #[tool(description = "Stop the currently running Scribe timer.")]
    async fn timer_stop(&self) -> String {
        match self.tracker.stop_timer() {
            Ok(entry) => to_json(&entry),
            Err(e) => err_json(e),
        }
    }

    /// Returns the running timer and its elapsed seconds.
    #[tool(description = "Get the active Scribe timer status (slug, project, elapsed seconds).")]
    async fn timer_status(&self) -> String {
        match self.tracker.timer_status() {
            Ok(Some((entry, elapsed))) => to_json(&serde_json::json!({
                "slug": entry.slug,
                "project_id": entry.project_id,
                "task_id": entry.task_id,
                "started_at": entry.started_at,
                "elapsed_seconds": elapsed.num_seconds(),
                "note": entry.note,
            })),
            Ok(None) => "null".to_owned(),
            Err(e) => err_json(e),
        }
    }

    /// Returns a time-tracking report.
    #[tool(
        description = "Generate a Scribe time report. period: 'today', 'week', or omit for all-time."
    )]
    async fn track_report(&self, Parameters(p): Parameters<TrackReportParams>) -> String {
        use chrono::{DateTime, Duration, TimeZone as _, Utc};

        let project_id = if let Some(ref slug) = p.project_slug {
            match self.projects.get_project(slug) {
                Ok(Some(pr)) => Some(pr.id),
                Ok(None) => return err_json(format!("project '{slug}' not found")),
                Err(e) => return err_json(e),
            }
        } else {
            None
        };

        let now = Utc::now();
        // DOCUMENTED-MAGIC: Epoch is 0 = "all-time" lower bound; the upper bound
        // is now+1s to include entries started in the current second.
        let (since, until) = match p.period.as_deref() {
            Some("today") => {
                let start = Utc
                    .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
                    .single()
                    .unwrap_or(now - Duration::hours(24));
                (start, now + Duration::seconds(1))
            }
            Some("week") => {
                let days_since_monday = i64::from(now.weekday().num_days_from_monday());
                let week_start = now - Duration::days(days_since_monday);
                let start = Utc
                    .with_ymd_and_hms(
                        week_start.year(),
                        week_start.month(),
                        week_start.day(),
                        0,
                        0,
                        0,
                    )
                    .single()
                    .unwrap_or(now - Duration::days(7));
                (start, now + Duration::seconds(1))
            }
            _ => (
                // Unix epoch as the "all-time" start sentinel.
                DateTime::from_timestamp(0, 0).unwrap_or(now - Duration::days(3650)),
                now + Duration::seconds(1),
            ),
        };

        match self.tracker.report(project_id, since, until) {
            Ok(entries) => {
                let rows: Vec<_> = entries
                    .into_iter()
                    .map(|(entry, dur)| {
                        serde_json::json!({
                            "slug": entry.slug,
                            "project_id": entry.project_id,
                            "task_id": entry.task_id,
                            "started_at": entry.started_at,
                            "ended_at": entry.ended_at,
                            "duration_seconds": dur.num_seconds(),
                            "note": entry.note,
                        })
                    })
                    .collect();
                to_json(&rows)
            }
            Err(e) => err_json(e),
        }
    }

    // ── Capture / inbox ─────────────────────────────────────────────────────

    /// Quick-captures a thought into the inbox.
    #[tool(description = "Quick-capture a thought into the Scribe inbox (no project required).")]
    async fn capture(&self, Parameters(p): Parameters<CaptureParams>) -> String {
        match self.inbox.capture(&p.body) {
            Ok(item) => to_json(&item),
            Err(e) => err_json(e),
        }
    }

    /// Lists unprocessed capture items.
    #[tool(description = "List unprocessed items in the Scribe inbox.")]
    async fn inbox_list(&self) -> String {
        match self.inbox.list(false) {
            Ok(items) => to_json(&items),
            Err(e) => err_json(e),
        }
    }

    /// Processes a capture item.
    #[tool(
        description = "Process a Scribe inbox item. action: 'task', 'todo', 'assign', or 'discard'."
    )]
    async fn inbox_process(&self, Parameters(p): Parameters<InboxProcessParams>) -> String {
        use crate::ops::inbox::ProcessAction;

        let action = match p.action.as_str() {
            "task" => {
                let Some(project_slug) = p.project_slug else {
                    return err_json("project_slug required for action 'task'");
                };
                ProcessAction::ConvertToTask {
                    project_slug,
                    title: p.title,
                    priority: None,
                }
            }
            "todo" => {
                let Some(project_slug) = p.project_slug else {
                    return err_json("project_slug required for action 'todo'");
                };
                ProcessAction::ConvertToTodo {
                    project_slug,
                    title: p.title,
                }
            }
            "assign" => {
                let Some(project_slug) = p.project_slug else {
                    return err_json("project_slug required for action 'assign'");
                };
                ProcessAction::AssignToProject { project_slug }
            }
            "discard" => ProcessAction::Discard,
            other => {
                return err_json(format!(
                    "unknown action '{other}'; use task, todo, assign, or discard"
                ));
            }
        };

        match self.inbox.process(&p.slug, action) {
            Ok(item) => to_json(&item),
            Err(e) => err_json(e),
        }
    }

    // ── Reminders ───────────────────────────────────────────────────────────

    /// Lists active reminders.
    #[tool(description = "List active Scribe reminders, optionally filtered by project_slug.")]
    async fn reminder_list(&self, Parameters(p): Parameters<ReminderListParams>) -> String {
        let project_id = if let Some(ref slug) = p.project_slug {
            match self.projects.get_project(slug) {
                Ok(Some(pr)) => Some(pr.id),
                Ok(None) => return err_json(format!("project '{slug}' not found")),
                Err(e) => return err_json(e),
            }
        } else {
            None
        };

        match self.reminders.list(project_id, false) {
            Ok(reminders) => to_json(&reminders),
            Err(e) => err_json(e),
        }
    }

    /// Creates a new reminder.
    #[tool(
        description = "Create a Scribe reminder. 'at' accepts flexible datetime formats (see GUIDE.md)."
    )]
    async fn reminder_create(&self, Parameters(p): Parameters<ReminderCreateParams>) -> String {
        use crate::cli::parse::parse_datetime;
        use crate::ops::reminders::CreateReminder;

        let remind_at = match parse_datetime(&p.at) {
            Ok(dt) => dt,
            Err(e) => return err_json(format!("invalid datetime '{}': {e}", p.at)),
        };

        match self.reminders.create(CreateReminder {
            project_slug: p.project_slug,
            task_slug: p.task_slug,
            remind_at,
            message: p.message,
            persistent: p.persistent.unwrap_or(false),
        }) {
            Ok(reminder) => to_json(&reminder),
            Err(e) => err_json(e),
        }
    }

    /// Archives a reminder.
    #[tool(description = "Archive a Scribe reminder.")]
    async fn reminder_archive(&self, Parameters(p): Parameters<ReminderSlugParam>) -> String {
        match self.reminders.archive(&p.slug) {
            Ok(reminder) => to_json(&reminder),
            Err(e) => err_json(e),
        }
    }
}

// ── ServerHandler impl ──────────────────────────────────────────────────────

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ScribeMcpServer {
    fn get_info(&self) -> ServerInfo {
        use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities};
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        ServerInfo::new(capabilities)
            .with_protocol_version(ProtocolVersion::LATEST)
            .with_server_info(Implementation::new("scribe", env!("CARGO_PKG_VERSION")))
    }

    fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, rmcp::ErrorData>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        let resources: Vec<rmcp::model::Resource> = RESOURCE_URIS
            .iter()
            .map(|(uri, desc)| rmcp::model::RawResource::new(*uri, *desc).no_annotation())
            .collect();
        std::future::ready(Ok(ListResourcesResult::with_all_items(resources)))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, rmcp::ErrorData>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        let contents = read_resource_contents(self, &request.uri);
        std::future::ready(Ok(ReadResourceResult::new(vec![contents])))
    }
}

// ── Resources ───────────────────────────────────────────────────────────────

/// All exposed resource URIs and their human-readable descriptions.
// DOCUMENTED-MAGIC: Six URIs form the complete read interface of Scribe data;
// the `scribe://` scheme is Scribe-specific and not IANA-registered.
const RESOURCE_URIS: &[(&str, &str)] = &[
    ("scribe://projects", "All active Scribe projects"),
    ("scribe://tasks/active", "All active non-archived tasks"),
    ("scribe://todos/active", "All active non-done todos"),
    (
        "scribe://timer/active",
        "The currently running timer, or null",
    ),
    ("scribe://inbox", "Unprocessed capture items"),
    ("scribe://reminders/pending", "Active unfired reminders"),
];

/// Reads the resource identified by `uri` and returns [`ResourceContents`].
fn read_resource_contents(server: &ScribeMcpServer, uri: &str) -> ResourceContents {
    let text = match uri {
        "scribe://projects" => match server.projects.list_projects(None, false) {
            Ok(v) => to_json(&v),
            Err(e) => err_json(e),
        },
        "scribe://tasks/active" => match server.tasks.list_tasks(None, None, None, false) {
            Ok(v) => to_json(&v),
            Err(e) => err_json(e),
        },
        "scribe://todos/active" => match server.todos.list(None, false, false) {
            Ok(v) => to_json(&v),
            Err(e) => err_json(e),
        },
        "scribe://timer/active" => match server.tracker.timer_status() {
            Ok(Some((entry, elapsed))) => to_json(&serde_json::json!({
                "slug": entry.slug,
                "project_id": entry.project_id,
                "started_at": entry.started_at,
                "elapsed_seconds": elapsed.num_seconds(),
                "note": entry.note,
            })),
            Ok(None) => "null".to_owned(),
            Err(e) => err_json(e),
        },
        "scribe://inbox" => match server.inbox.list(false) {
            Ok(v) => to_json(&v),
            Err(e) => err_json(e),
        },
        "scribe://reminders/pending" => match server.reminders.list(None, false) {
            Ok(v) => to_json(&v),
            Err(e) => err_json(e),
        },
        other => err_json(format!("unknown resource URI '{other}'")),
    };

    ResourceContents::text(text, uri)
}

// ── Testing utilities ────────────────────────────────────────────────────────

#[cfg(feature = "test-util")]
pub mod testing {
    //! Test helpers for the MCP server.
    //!
    //! This module exposes internals for integration testing of the MCP tool
    //! handlers without requiring the full stdio transport.

    use std::sync::{Arc, Mutex};

    use rusqlite::Connection;

    /// A test harness that wraps an in-memory database and MCP server.
    #[derive(Debug)]
    pub struct TestMcpServer {
        /// Shared database connection (in-memory).
        pub conn: Arc<Mutex<Connection>>,
        /// The MCP server instance.
        pub server: super::ScribeMcpServer,
    }

    impl TestMcpServer {
        /// Creates a new test MCP server with an in-memory database.
        ///
        /// # Panics
        ///
        /// Panics if the in-memory database cannot be opened (should not happen).
        #[must_use]
        pub fn new() -> Self {
            let conn = Arc::new(Mutex::new(
                rusqlite::Connection::open_in_memory().expect("in-memory database should open"),
            ));
            let server = super::ScribeMcpServer::new(&conn);
            Self { conn, server }
        }
    }

    impl Default for TestMcpServer {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

/// Creates the MCP server and runs it on the stdio transport until the client disconnects.
///
/// # Errors
///
/// Returns an error if the MCP server fails during initialisation or if the
/// stdio transport encounters an I/O error.
pub async fn serve(conn: &Arc<Mutex<Connection>>) -> anyhow::Result<()> {
    let server = ScribeMcpServer::new(conn);
    let transport = rmcp::transport::stdio();
    server
        .serve(transport)
        .await
        .map_err(|e| anyhow::anyhow!("MCP server error: {e}"))?
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("MCP server task error: {e}"))?;
    Ok(())
}
