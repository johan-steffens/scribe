// Rust guideline compliant 2026-02-21
//! Private handler functions for `scribe reminder` subcommands.
//!
//! This file is included by `reminder.rs` via `#[path = "reminder_handlers.rs"]`.

use serde_json::json;

use super::{
    OutputFormat, ReminderAdd, ReminderArchive, ReminderDelete, ReminderList, ReminderRestore,
    ReminderShow,
};
use crate::cli::parse::parse_datetime;
use crate::ops::reminders::CreateReminder;
use crate::ops::{ProjectOps, ReminderOps};

pub(super) fn handle_add(args: &ReminderAdd, ops: &ReminderOps) -> anyhow::Result<()> {
    let remind_at = parse_datetime(&args.at)?;
    let reminder = ops.create(CreateReminder {
        project_slug: args.project.clone(),
        task_slug: args.task.clone(),
        remind_at,
        message: args.message.clone(),
    })?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&reminder)?),
        OutputFormat::Text => println!(
            "Created reminder: {} (fires at {})",
            reminder.slug,
            reminder.remind_at.format("%Y-%m-%d %H:%M UTC"),
        ),
    }
    Ok(())
}

pub(super) fn handle_list(
    args: &ReminderList,
    ops: &ReminderOps,
    project_ops: &ProjectOps,
) -> anyhow::Result<()> {
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

    let reminders = ops.list(project_id, args.archived)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&reminders)?),
        OutputFormat::Text => {
            if reminders.is_empty() {
                println!("No reminders found.");
            } else {
                for r in &reminders {
                    let fired = if r.fired { " [fired]" } else { "" };
                    let archived = if r.archived_at.is_some() {
                        " [archived]"
                    } else {
                        ""
                    };
                    println!(
                        "{:<45} {}  {}{}{}",
                        r.slug,
                        r.remind_at.format("%Y-%m-%d %H:%M"),
                        r.message.as_deref().unwrap_or(""),
                        fired,
                        archived,
                    );
                }
            }
        }
    }
    Ok(())
}

pub(super) fn handle_show(args: &ReminderShow, ops: &ReminderOps) -> anyhow::Result<()> {
    let reminder = ops
        .get(&args.slug)?
        .ok_or_else(|| anyhow::anyhow!("reminder '{}' not found", args.slug))?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&reminder)?),
        OutputFormat::Text => {
            println!("slug:      {}", reminder.slug);
            println!(
                "remind_at: {}",
                reminder.remind_at.format("%Y-%m-%d %H:%M UTC")
            );
            println!("fired:     {}", reminder.fired);
            println!("message:   {}", reminder.message.as_deref().unwrap_or("—"));
            println!(
                "created:   {}",
                reminder.created_at.format("%Y-%m-%d %H:%M UTC")
            );
            if let Some(at) = reminder.archived_at {
                println!("archived:  {}", at.format("%Y-%m-%d %H:%M UTC"));
            }
        }
    }
    Ok(())
}

pub(super) fn handle_archive(args: &ReminderArchive, ops: &ReminderOps) -> anyhow::Result<()> {
    let reminder = ops.archive(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&reminder)?),
        OutputFormat::Text => println!("Archived reminder: {}", reminder.slug),
    }
    Ok(())
}

pub(super) fn handle_restore(args: &ReminderRestore, ops: &ReminderOps) -> anyhow::Result<()> {
    let reminder = ops.restore(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&reminder)?),
        OutputFormat::Text => println!("Restored reminder: {}", reminder.slug),
    }
    Ok(())
}

pub(super) fn handle_delete(args: &ReminderDelete, ops: &ReminderOps) -> anyhow::Result<()> {
    ops.delete(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", json!({ "deleted": args.slug })),
        OutputFormat::Text => println!("Deleted reminder: {}", args.slug),
    }
    Ok(())
}
