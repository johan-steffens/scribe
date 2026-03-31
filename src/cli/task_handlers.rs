// Rust guideline compliant 2026-02-21
//! Private handler functions for `scribe task` subcommands.
//!
//! This file is included by `task.rs` via `#[path = "task_handlers.rs"]`.

use serde_json::json;

use super::{
    OutputFormat, TaskAdd, TaskArchive, TaskDelete, TaskDone, TaskEdit, TaskList, TaskMove,
    TaskRestore, TaskShow,
};
use crate::domain::{TaskPatch, TaskStatus};
use crate::ops::tasks::CreateTask;
use crate::ops::{ProjectOps, TaskOps};

pub(super) fn parse_due_date(s: &str) -> anyhow::Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("invalid date '{s}': {e}"))
}

pub(super) fn handle_add(
    args: &TaskAdd,
    task_ops: &TaskOps,
    project_ops: &ProjectOps,
) -> anyhow::Result<()> {
    let project_slug = args
        .project
        .clone()
        .unwrap_or_else(|| "quick-capture".to_owned());
    let project = project_ops
        .get_project(&project_slug)?
        .ok_or_else(|| anyhow::anyhow!("project '{project_slug}' not found"))?;

    let due_date = args.due.as_deref().map(parse_due_date).transpose()?;

    let task = task_ops.create_task(CreateTask {
        project_slug: project.slug.clone(),
        project_id: project.id,
        title: args.title.clone(),
        description: None,
        status: TaskStatus::Todo,
        priority: args.priority,
        due_date,
    })?;

    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Created task: {} [{}]", task.title, task.slug),
    }
    Ok(())
}

pub(super) fn handle_list(
    args: &TaskList,
    task_ops: &TaskOps,
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

    let tasks = task_ops.list_tasks(project_id, args.status, args.priority, args.archived)?;

    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&tasks)?),
        OutputFormat::Text => {
            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                for t in &tasks {
                    let archived = if t.archived_at.is_some() {
                        " [archived]"
                    } else {
                        ""
                    };
                    println!("{:<45} [{}] [{}]{}", t.slug, t.status, t.priority, archived);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn handle_show(args: &TaskShow, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops
        .get_task(&args.slug)?
        .ok_or_else(|| anyhow::anyhow!("task '{}' not found", args.slug))?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => {
            println!("slug:        {}", task.slug);
            println!("title:       {}", task.title);
            println!("status:      {}", task.status);
            println!("priority:    {}", task.priority);
            println!(
                "due:         {}",
                task.due_date
                    .map(|d| d.to_string())
                    .as_deref()
                    .unwrap_or("—")
            );
            println!(
                "description: {}",
                task.description.as_deref().unwrap_or("—")
            );
            println!(
                "created:     {}",
                task.created_at.format("%Y-%m-%d %H:%M UTC")
            );
        }
    }
    Ok(())
}

pub(super) fn handle_edit(args: &TaskEdit, task_ops: &TaskOps) -> anyhow::Result<()> {
    let due_date = args.due.as_deref().map(parse_due_date).transpose()?;

    let patch = TaskPatch {
        title: args.title.clone(),
        description: None,
        clear_description: false,
        status: args.status,
        priority: args.priority,
        due_date,
        clear_due_date: false,
        project_id: None,
    };
    let task = task_ops.update_task(&args.slug, patch)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Updated task: {} [{}]", task.title, task.slug),
    }
    Ok(())
}

pub(super) fn handle_move(
    args: &TaskMove,
    task_ops: &TaskOps,
    project_ops: &ProjectOps,
) -> anyhow::Result<()> {
    let project = project_ops
        .get_project(&args.project)?
        .ok_or_else(|| anyhow::anyhow!("project '{}' not found", args.project))?;
    let task = task_ops.update_task(
        &args.slug,
        TaskPatch {
            project_id: Some(project.id),
            ..Default::default()
        },
    )?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => {
            println!("Moved task '{}' to project '{}'", task.slug, args.project);
        }
    }
    Ok(())
}

pub(super) fn handle_done(args: &TaskDone, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops.mark_done(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Done: {}", task.slug),
    }
    Ok(())
}

pub(super) fn handle_archive(args: &TaskArchive, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops.archive_task(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Archived task: {}", task.slug),
    }
    Ok(())
}

pub(super) fn handle_restore(args: &TaskRestore, task_ops: &TaskOps) -> anyhow::Result<()> {
    let task = task_ops.restore_task(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&task)?),
        OutputFormat::Text => println!("Restored task: {}", task.slug),
    }
    Ok(())
}

pub(super) fn handle_delete(args: &TaskDelete, task_ops: &TaskOps) -> anyhow::Result<()> {
    task_ops.delete_task(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", json!({ "deleted": args.slug })),
        OutputFormat::Text => println!("Deleted task: {}", args.slug),
    }
    Ok(())
}
