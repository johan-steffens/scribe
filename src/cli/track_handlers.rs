//! Private handler functions for `scribe track` subcommands.
//!
//! This file is included by `track.rs` via `#[path = "track_handlers.rs"]`.

use chrono::{Duration, Local, Utc};
use serde_json::json;

use super::{OutputFormat, TrackReport, TrackStart, TrackStatus, TrackStop};
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

pub(super) fn handle_report(
    args: &TrackReport,
    ops: &TrackerOps,
    project_ops: &ProjectOps,
) -> anyhow::Result<()> {
    let now = Utc::now();

    // Determine the time window.
    let (since, until) = if args.today {
        // Midnight local time today → now.
        let midnight = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid midnight")
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| anyhow::anyhow!("failed to compute local midnight"))?
            .with_timezone(&Utc);
        (midnight, now + Duration::seconds(1))
    } else if args.week {
        // Monday midnight local → now.
        use chrono::Datelike;
        let today = Local::now().date_naive();
        let days_since_monday = today.weekday().num_days_from_monday();
        let monday = today - Duration::days(i64::from(days_since_monday));
        let monday_dt = monday
            .and_hms_opt(0, 0, 0)
            .expect("valid midnight")
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| anyhow::anyhow!("failed to compute Monday midnight"))?
            .with_timezone(&Utc);
        (monday_dt, now + Duration::seconds(1))
    } else {
        // Default: all time (epoch to far future).
        let epoch = chrono::DateTime::UNIX_EPOCH;
        (epoch, now + Duration::days(365 * 100))
    };

    let project_id = args
        .project
        .as_deref()
        .map(|slug| {
            project_ops
                .get_project(slug)?
                .ok_or_else(|| anyhow::anyhow!("project '{slug}' not found"))
                .map(|p| p.id)
        })
        .transpose()?;

    let entries = ops.report(project_id, since, until)?;

    match args.output {
        OutputFormat::Json => {
            let items: Vec<_> = entries
                .iter()
                .map(|(e, d)| {
                    json!({
                        "slug": e.slug,
                        "started_at": e.started_at,
                        "ended_at": e.ended_at,
                        "duration_seconds": d.num_seconds(),
                        "note": e.note,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items)?);
        }
        OutputFormat::Text => {
            if entries.is_empty() {
                println!("No time entries found.");
            } else {
                let total: Duration = entries.iter().map(|(_, d)| *d).sum();
                for (e, d) in &entries {
                    let mins = d.num_minutes();
                    let secs = d.num_seconds() % 60;
                    let note = e.note.as_deref().unwrap_or("");
                    println!("{:<45} {mins:>4}m {secs:02}s  {}", e.slug, note,);
                }
                let total_mins = total.num_minutes();
                let total_secs = total.num_seconds() % 60;
                println!("─────────────────────────────────────────────");
                println!("Total: {total_mins}m {total_secs}s");
            }
        }
    }
    Ok(())
}
