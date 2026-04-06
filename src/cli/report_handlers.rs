//! Handler functions for `scribe report` subcommands.
//!
//! This module provides the implementation for rendering summary and
//! domain-specific reports in both text and JSON formats.

use std::sync::{Arc, Mutex};

use chrono::{Duration, Local, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::cli::project::OutputFormat;
use crate::cli::report::{ReportCommand, ReportSubcommand};
use crate::ops::reporting::{ProjectReport, ReportingOps, SummaryReport, TaskReport};

/// The separator line used in text-mode reports.
const SEPARATOR: &str = "────────────────────────────────────────────────────────────";

/// Formats a `Duration` as "Xh Ym" (e.g., "6h 42m").
fn format_duration(d: Duration) -> String {
    let total_mins = d.num_minutes();
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

/// Determines the time window based on `--today` / `--week` flags.
///
/// Returns `(since, until)` as UTC datetimes.
fn compute_time_window(today: bool, week: bool) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let now = Utc::now();
    if today {
        // Midnight local time today → now.
        let midnight = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid midnight")
            .and_local_timezone(Local)
            .single()
            .expect("unique local midnight")
            .with_timezone(&Utc);
        (midnight, now + Duration::seconds(1))
    } else if week {
        // Monday midnight local → now.
        use chrono::Datelike;
        let today_date = Local::now().date_naive();
        let days_since_monday = today_date.weekday().num_days_from_monday();
        let monday = today_date - Duration::days(i64::from(days_since_monday));
        let monday_dt = monday
            .and_hms_opt(0, 0, 0)
            .expect("valid midnight")
            .and_local_timezone(Local)
            .single()
            .expect("unique local midnight")
            .with_timezone(&Utc);
        (monday_dt, now + Duration::seconds(1))
    } else {
        // Default: all time (epoch to far future).
        let epoch = chrono::DateTime::UNIX_EPOCH;
        (epoch, now + Duration::days(365 * 100))
    }
}

// ── public API ────────────────────────────────────────────────────────────────

/// Dispatches to the appropriate handler based on `cmd.subcommand`.
///
/// # Errors
///
/// Returns an error if the database query fails or the requested entity is not found.
pub fn handle_report(cmd: &ReportCommand, conn: Arc<Mutex<Connection>>) -> anyhow::Result<()> {
    let ops = ReportingOps::new(conn);

    match &cmd.subcommand {
        None => handle_inbox_report_impl(cmd, &ops, &cmd.output, cmd.detailed),
        Some(ReportSubcommand::Inbox { common }) => {
            handle_inbox_report_impl(cmd, &ops, &common.output, common.detailed)
        }
        Some(ReportSubcommand::Reminders { common }) => {
            handle_reminders_report_impl(cmd, &ops, &common.output, common.detailed)
        }
        Some(ReportSubcommand::Project { slug, common }) => {
            handle_project_report_impl(cmd, &ops, slug, &common.output, common.detailed)
        }
        Some(ReportSubcommand::Task { slug, common }) => {
            handle_task_report_impl(cmd, &ops, slug, &common.output, common.detailed)
        }
        Some(ReportSubcommand::Todo { slug, common }) => {
            handle_todo_report_impl(cmd, &ops, slug, &common.output, common.detailed)
        }
    }
}

// ── summary helpers ───────────────────────────────────────────────────────────

fn handle_inbox_report_impl(
    cmd: &ReportCommand,
    ops: &ReportingOps,
    output: &OutputFormat,
    _detailed: bool,
) -> anyhow::Result<()> {
    let (since, until) = compute_time_window(cmd.today, cmd.week);
    let report = ops.summary_report(since, until)?;

    match output {
        OutputFormat::Json => {
            let payload = InboxReportPayload {
                unprocessed_items: report.items_in_inbox,
                since,
                until,
            };
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Text => {
            println!("Inbox Status Report");
            println!("{SEPARATOR}");
            println!("Unprocessed Items: {}", report.items_in_inbox);
            println!("{SEPARATOR}");
        }
    }
    Ok(())
}

fn handle_reminders_report_impl(
    cmd: &ReportCommand,
    ops: &ReportingOps,
    output: &OutputFormat,
    _detailed: bool,
) -> anyhow::Result<()> {
    let (since, until) = compute_time_window(cmd.today, cmd.week);
    let report = ops.summary_report(since, until)?;

    // TODO: Fetch upcoming/past-due reminders from ReminderOps when available
    match output {
        OutputFormat::Json => {
            let payload = RemindersReportPayload {
                active_reminders: report.active_reminders,
                since,
                until,
            };
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Text => {
            println!("Reminders Status Report");
            println!("{SEPARATOR}");
            println!("Active Reminders: {}", report.active_reminders);
            println!("{SEPARATOR}");
        }
    }
    Ok(())
}

/// Payload struct for JSON serialization of inbox reports.
#[derive(Serialize)]
struct InboxReportPayload {
    unprocessed_items: usize,
    since: chrono::DateTime<Utc>,
    until: chrono::DateTime<Utc>,
}

/// Payload struct for JSON serialization of reminders reports.
#[derive(Serialize)]
struct RemindersReportPayload {
    active_reminders: usize,
    since: chrono::DateTime<Utc>,
    until: chrono::DateTime<Utc>,
}

// ── per-domain handlers ───────────────────────────────────────────────────────

fn handle_project_report_impl(
    cmd: &ReportCommand,
    ops: &ReportingOps,
    slug: &str,
    output: &OutputFormat,
    detailed: bool,
) -> anyhow::Result<()> {
    let (since, until) = compute_time_window(cmd.today, cmd.week);
    let report = ops.project_report(slug, since, until)?;

    match output {
        OutputFormat::Json => {
            let payload = ProjectReportPayload {
                project: &report.project,
                pending_tasks: &report.pending_tasks,
                open_todos: &report.open_todos,
                total_time: report.total_time,
                completion_percentage: report.completion_percentage,
                since,
                until,
            };
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Text => {
            let date_range = if cmd.today {
                "today"
            } else if cmd.week {
                "this week"
            } else {
                "all time"
            };
            if detailed {
                print_detailed_project_report(&report, date_range);
            } else {
                print_compact_project_report(&report, date_range);
            }
        }
    }
    Ok(())
}

fn print_compact_project_report(report: &ProjectReport, date_range: &str) {
    println!(
        "Project Report: {} ({})",
        report.project.name, report.project.slug
    );
    println!(
        "Status: {} | Created: {}",
        report.project.status,
        report.project.created_at.format("%Y-%m-%d")
    );
    println!("{SEPARATOR}");

    if report.pending_tasks.is_empty() {
        println!("Pending Tasks: none");
    } else {
        println!("Pending Tasks:");
        for task in &report.pending_tasks {
            println!("- {} ({})", task.title, task.priority);
        }
    }

    if report.open_todos.is_empty() {
        println!("\nOpen Todos: none");
    } else {
        println!("\nOpen Todos:");
        for todo in &report.open_todos {
            println!("- {}", todo.title);
        }
    }

    if report.time_entries.is_empty() {
        println!("\nTracked Time: none");
    } else {
        println!("\nTracked Time:");
        for (entry, duration) in &report.time_entries {
            println!("- {}    {}", entry.slug, format_duration(*duration));
        }
    }

    println!("{SEPARATOR}");
    println!("Total Project Time: {}", format_duration(report.total_time));
    // Note: completion_percentage already accounts for all tasks in project_report
    println!("Completion: {:.0}%", report.completion_percentage);
    println!("({date_range})");
}

fn print_detailed_project_report(report: &ProjectReport, date_range: &str) {
    println!(
        "Detailed Project Report: {} ({})",
        report.project.name, report.project.slug
    );
    println!(
        "Status: {} | Created: {} | Updated: {}",
        report.project.status,
        report.project.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
        report.project.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(desc) = &report.project.description {
        println!("Description: {desc}");
    }
    println!("{SEPARATOR}");

    if report.pending_tasks.is_empty() {
        println!("Pending Tasks: none");
    } else {
        println!("Pending Tasks (Detailed):");
        for task in &report.pending_tasks {
            println!("- {} ({})", task.title, task.priority);
            println!(
                "  Status: {} | Created: {}",
                task.status,
                task.created_at.format("%Y-%m-%d")
            );
            if let Some(note) = &task.description {
                println!("  Notes: {note}");
            }
        }
    }

    if report.open_todos.is_empty() {
        println!("\nOpen Todos: none");
    } else {
        println!("\nOpen Todos (Detailed):");
        for todo in &report.open_todos {
            println!(
                "- [ ] {} (Created: {})",
                todo.title,
                todo.created_at.format("%Y-%m-%d")
            );
        }
    }

    if report.time_entries.is_empty() {
        println!("\nDetailed Tracked Time: none");
    } else {
        println!("\nDetailed Tracked Time:");
        for (entry, duration) in &report.time_entries {
            let started = entry.started_at.format("%Y-%m-%d %H:%M:%S");
            let ended = entry.ended_at.map_or_else(
                || "in progress".to_string(),
                |e| e.format("%Y-%m-%d %H:%M:%S").to_string(),
            );
            println!(
                "- {} to {} ({})",
                started,
                ended,
                format_duration(*duration)
            );
            if let Some(note) = &entry.note {
                println!("  Note: {note}");
            }
        }
    }

    println!("{SEPARATOR}");
    println!("Total Project Time: {}", format_duration(report.total_time));
    println!(
        "Completion: {:.1}% ({} tasks done, {} pending)",
        report.completion_percentage,
        report
            .pending_tasks
            .iter()
            .filter(|t| t.status == crate::domain::TaskStatus::Done)
            .count(),
        report.pending_tasks.len()
    );
    println!("({date_range})");
}

/// Payload struct for JSON serialization of project reports.
#[derive(Serialize)]
struct ProjectReportPayload<'a> {
    project: &'a crate::domain::Project,
    pending_tasks: &'a Vec<crate::domain::Task>,
    open_todos: &'a Vec<crate::domain::Todo>,
    total_time: Duration,
    completion_percentage: f32,
    since: chrono::DateTime<Utc>,
    until: chrono::DateTime<Utc>,
}

fn handle_task_report_impl(
    _cmd: &ReportCommand,
    ops: &ReportingOps,
    slug: &str,
    output: &OutputFormat,
    detailed: bool,
) -> anyhow::Result<()> {
    let report = ops.task_report(slug)?;

    match output {
        OutputFormat::Json => {
            let payload = TaskReportPayload {
                task: &report.task,
                time_entries: &report.time_entries,
                total_time: report.total_time,
            };
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Text => {
            if detailed {
                print_detailed_task_report(&report);
            } else {
                print_compact_task_report(&report);
            }
        }
    }
    Ok(())
}

fn print_compact_task_report(report: &TaskReport) {
    println!("Task Report: {} ({})", report.task.title, report.task.slug);
    println!(
        "Project: {} | Priority: {} | Status: {}",
        report.task.project_slug, report.task.priority, report.task.status
    );
    println!("{SEPARATOR}");

    println!("Lifecycle:");
    println!(
        "- {}: Created",
        report.task.created_at.format("%Y-%m-%d %H:%M")
    );
    // Note: status changes are not tracked in the current Task model

    if report.time_entries.is_empty() {
        println!("\nRelated Time Tracking: none");
    } else {
        println!("\nRelated Time Tracking:");
        for (entry, duration) in &report.time_entries {
            println!("- {}    {}", entry.slug, format_duration(*duration));
        }
    }

    println!("{SEPARATOR}");
    println!("Total Task Time: {}", format_duration(report.total_time));
}

fn print_detailed_task_report(report: &TaskReport) {
    println!("Task Report: {} ({})", report.task.title, report.task.slug);
    println!(
        "Project: {} | Priority: {} | Status: {}",
        report.task.project_slug, report.task.priority, report.task.status
    );
    if let Some(due) = report.task.due_date {
        println!("Due: {due}");
    }
    if let Some(desc) = &report.task.description {
        println!("Description: {desc}");
    }
    println!("{SEPARATOR}");

    println!("Lifecycle:");
    println!(
        "- {}: Created",
        report.task.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    if report.time_entries.is_empty() {
        println!("\nRelated Time Tracking: none");
    } else {
        println!("\nDetailed Tracked Time:");
        for (entry, duration) in &report.time_entries {
            let started = entry.started_at.format("%Y-%m-%d %H:%M:%S");
            let ended = entry.ended_at.map_or_else(
                || "in progress".to_string(),
                |e| e.format("%Y-%m-%d %H:%M:%S").to_string(),
            );
            println!(
                "- {} to {} ({})",
                started,
                ended,
                format_duration(*duration)
            );
            if let Some(note) = &entry.note {
                println!("  Note: {note}");
            }
        }
    }

    println!("{SEPARATOR}");
    println!("Total Task Time: {}", format_duration(report.total_time));
}

/// Payload struct for JSON serialization of task reports.
#[derive(Serialize)]
struct TaskReportPayload<'a> {
    task: &'a crate::domain::Task,
    time_entries: &'a Vec<(crate::domain::TimeEntry, Duration)>,
    total_time: Duration,
}

fn handle_todo_report_impl(
    _cmd: &ReportCommand,
    _ops: &ReportingOps,
    _slug: &str,
    output: &OutputFormat,
    _detailed: bool,
) -> anyhow::Result<()> {
    // TODO: Implement todo-specific report when TodoReport is available in ReportingOps
    match output {
        OutputFormat::Json => {
            let payload = serde_json::json!({
                "message": "todo-specific reports not yet implemented"
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Text => {
            println!("Todo-specific reports are not yet implemented.");
            println!("Use `scribe todo list` to see all todos.");
        }
    }
    Ok(())
}

/// Main summary report handler (used when no subcommand is provided).
///
/// This handler is called from `handle_report` when `cmd.subcommand` is `None`.
/// Note: The actual summary report is rendered inside the match arms above
/// (e.g., `handle_inbox_report`) since each domain has its own report structure.
/// For a true global summary, use `ops.summary_report()` directly.
#[allow(
    dead_code,
    reason = "summary_report handler is reserved for future use when summary subcommand is added"
)]
fn handle_summary_report(cmd: &ReportCommand, ops: &ReportingOps) -> anyhow::Result<()> {
    let (since, until) = compute_time_window(cmd.today, cmd.week);
    let report = ops.summary_report(since, until)?;

    let date_label = if cmd.today {
        Utc::now().format("%Y-%m-%d").to_string()
    } else if cmd.week {
        "this week".to_string()
    } else {
        "all time".to_string()
    };

    match cmd.output {
        OutputFormat::Json => {
            let payload = SummaryReportPayload {
                active_projects: report.active_projects,
                pending_tasks: report.pending_tasks,
                open_todos: report.open_todos,
                items_in_inbox: report.items_in_inbox,
                active_reminders: report.active_reminders,
                total_time_tracked_seconds: report.total_time_tracked.num_seconds(),
                overdue_tasks: report.overdue_tasks,
                since,
                until,
                date_label,
            };
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Text => {
            if cmd.detailed {
                print_detailed_summary(&report, &date_label);
            } else {
                print_compact_summary(&report, &date_label);
            }
        }
    }
    Ok(())
}

/// Payload struct for JSON serialization of summary reports.
#[derive(Serialize)]
struct SummaryReportPayload {
    active_projects: usize,
    pending_tasks: usize,
    open_todos: usize,
    items_in_inbox: usize,
    active_reminders: usize,
    total_time_tracked_seconds: i64,
    overdue_tasks: usize,
    since: chrono::DateTime<Utc>,
    until: chrono::DateTime<Utc>,
    #[serde(skip_serializing_if = "String::is_empty")]
    date_label: String,
}

fn print_compact_summary(report: &SummaryReport, date_label: &str) {
    println!("Scribe Summary Report ({date_label})");
    println!("{SEPARATOR}");
    println!("Projects:     {} active", report.active_projects);
    println!(
        "Tasks:        {} pending, {} overdue",
        report.pending_tasks, report.overdue_tasks
    );
    println!("Todos:        {} open", report.open_todos);
    println!("Inbox:        {} unprocessed items", report.items_in_inbox);
    println!("Reminders:    {} active", report.active_reminders);
    println!(
        "Time Tracked: {}",
        format_duration(report.total_time_tracked)
    );
    println!("{SEPARATOR}");
}

fn print_detailed_summary(report: &SummaryReport, date_label: &str) {
    println!("Detailed Scribe Summary Report ({date_label})");
    println!("{SEPARATOR}");
    println!("Active Projects: {}", report.active_projects);
    println!(
        "Pending Tasks: {} ({} overdue)",
        report.pending_tasks, report.overdue_tasks
    );
    println!("Open Todos: {}", report.open_todos);
    println!("Unprocessed Inbox Items: {}", report.items_in_inbox);
    println!("Active Reminders: {}", report.active_reminders);
    println!(
        "Time Tracked: {}",
        format_duration(report.total_time_tracked)
    );
    println!("{SEPARATOR}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_shows_hours_and_minutes() {
        let d = Duration::hours(6) + Duration::minutes(42);
        assert_eq!(format_duration(d), "6h 42m");
    }

    #[test]
    fn format_duration_shows_minutes_only_when_no_hours() {
        let d = Duration::minutes(45);
        assert_eq!(format_duration(d), "45m");
    }

    #[test]
    fn format_duration_shows_zero_minutes() {
        let d = Duration::zero();
        assert_eq!(format_duration(d), "0m");
    }
}
