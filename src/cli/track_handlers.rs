//! Private handler functions for `scribe track` subcommands.
//!
//! This file is included by `track.rs` via `#[path = "track_handlers.rs"]`.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::json;

use super::{OutputFormat, TrackReport, TrackStart, TrackStatus, TrackStop};
use crate::cli::report::{ReportCommand, ReportSubcommand};
use crate::cli::report_handlers::handle_report as report_handle_report;
use crate::ops::tracker::StartTimer;
use crate::ops::{ProjectOps, TaskOps, TrackerOps};

pub(super) fn handle_start(
    args: &TrackStart,
    ops: &TrackerOps,
    task_ops: &TaskOps,
) -> anyhow::Result<()> {
    let project_slug = args
        .project
        .clone()
        .unwrap_or_else(|| "quick-capture".to_owned());

    let (slug, project_id) = ops.resolve_project(&project_slug)?;

    // Resolve optional task slug to a TaskId.
    let task_id = args
        .task
        .as_deref()
        .map(|task_slug| {
            task_ops
                .get_task(task_slug)?
                .ok_or_else(|| anyhow::anyhow!("task '{task_slug}' not found"))
                .map(|t| t.id)
        })
        .transpose()?;

    let entry = ops.start_timer(StartTimer {
        project_slug: slug,
        project_id,
        task_id,
        note: args.note.clone(),
    })?;

    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entry)?),
        OutputFormat::Text => println!("Started timer: {}", entry.slug),
    }
    Ok(())
}

pub(super) fn handle_stop(args: &TrackStop, ops: &TrackerOps) -> anyhow::Result<()> {
    let entry = ops.stop_timer()?;
    let duration = entry
        .ended_at
        .map(|e| e - entry.started_at)
        .unwrap_or_default();
    let mins = duration.num_minutes();
    let secs = duration.num_seconds() % 60;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entry)?),
        OutputFormat::Text => {
            println!("Stopped timer: {} ({mins}m {secs}s)", entry.slug,);
        }
    }
    Ok(())
}

pub(super) fn handle_status(args: &TrackStatus, ops: &TrackerOps) -> anyhow::Result<()> {
    match ops.timer_status()? {
        None => match args.output {
            OutputFormat::Json => println!("{}", json!({ "running": false })),
            OutputFormat::Text => println!("No timer running."),
        },
        Some((entry, elapsed)) => {
            let mins = elapsed.num_minutes();
            let secs = elapsed.num_seconds() % 60;
            match args.output {
                OutputFormat::Json => println!(
                    "{}",
                    json!({
                        "running": true,
                        "slug": entry.slug,
                        "elapsed_seconds": elapsed.num_seconds(),
                    })
                ),
                OutputFormat::Text => {
                    println!("Running: {} ({mins}m {secs}s)", entry.slug);
                    if let Some(note) = &entry.note {
                        println!("  note: {note}");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Delegates to the centralized [`report_handle_report`] handler.
///
/// This redirects `scribe track report` to the new centralized reporting system
/// via `ReportSubcommand::Track`.
pub(super) fn handle_report(
    args: &TrackReport,
    project_ops: &ProjectOps,
    conn: &Arc<Mutex<Connection>>,
) -> anyhow::Result<()> {
    let cmd = ReportCommand {
        subcommand: Some(ReportSubcommand::Track {
            today: args.today,
            week: args.week,
            project: args.project.clone(),
            output: args.output.clone(),
        }),
        today: args.today,
        week: args.week,
        output: args.output.clone(),
        detailed: false,
    };
    report_handle_report(&cmd, Arc::clone(conn), project_ops)
}
