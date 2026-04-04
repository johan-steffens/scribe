//! Private handler functions for `scribe todo` subcommands.
//!
//! This file is included by `todo.rs` via `#[path = "todo_handlers.rs"]`.

use serde_json::json;

use super::{
    OutputFormat, TodoAdd, TodoArchive, TodoDelete, TodoDone, TodoList, TodoMove, TodoRestore,
    TodoShow,
};
use crate::ops::{ProjectOps, TodoOps};

pub(super) fn handle_add(args: &TodoAdd, ops: &TodoOps) -> anyhow::Result<()> {
    let project_slug = args
        .project
        .clone()
        .unwrap_or_else(|| "quick-capture".to_owned());
    let todo = ops.create(&project_slug, &args.title)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&todo)?),
        OutputFormat::Text => println!("Created todo: {} [{}]", todo.title, todo.slug),
    }
    Ok(())
}

pub(super) fn handle_list(
    args: &TodoList,
    ops: &TodoOps,
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

    let todos = ops.list(project_id, args.all || args.archived, args.archived)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&todos)?),
        OutputFormat::Text => {
            if todos.is_empty() {
                println!("No todos found.");
            } else {
                for t in &todos {
                    let done = if t.done { " [done]" } else { "" };
                    let archived = if t.archived_at.is_some() {
                        " [archived]"
                    } else {
                        ""
                    };
                    println!("{:<45} {}{}{}", t.slug, t.title, done, archived);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn handle_show(args: &TodoShow, ops: &TodoOps) -> anyhow::Result<()> {
    let todo = ops
        .get(&args.slug)?
        .ok_or_else(|| anyhow::anyhow!("todo '{}' not found", args.slug))?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&todo)?),
        OutputFormat::Text => {
            println!("slug:    {}", todo.slug);
            println!("title:   {}", todo.title);
            println!("done:    {}", todo.done);
            println!("created: {}", todo.created_at.format("%Y-%m-%d %H:%M UTC"));
            if let Some(at) = todo.archived_at {
                println!("archived: {}", at.format("%Y-%m-%d %H:%M UTC"));
            }
        }
    }
    Ok(())
}

pub(super) fn handle_move(args: &TodoMove, ops: &TodoOps) -> anyhow::Result<()> {
    let todo = ops.move_project(&args.slug, &args.project)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&todo)?),
        OutputFormat::Text => {
            println!("Moved todo '{}' to project '{}'", todo.slug, args.project);
        }
    }
    Ok(())
}

pub(super) fn handle_done(args: &TodoDone, ops: &TodoOps) -> anyhow::Result<()> {
    let todo = ops.mark_done(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&todo)?),
        OutputFormat::Text => println!("Done: {}", todo.slug),
    }
    Ok(())
}

pub(super) fn handle_archive(args: &TodoArchive, ops: &TodoOps) -> anyhow::Result<()> {
    let todo = ops.archive(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&todo)?),
        OutputFormat::Text => println!("Archived todo: {}", todo.slug),
    }
    Ok(())
}

pub(super) fn handle_restore(args: &TodoRestore, ops: &TodoOps) -> anyhow::Result<()> {
    let todo = ops.restore(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&todo)?),
        OutputFormat::Text => println!("Restored todo: {}", todo.slug),
    }
    Ok(())
}

pub(super) fn handle_delete(args: &TodoDelete, ops: &TodoOps) -> anyhow::Result<()> {
    ops.delete(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", json!({ "deleted": args.slug })),
        OutputFormat::Text => println!("Deleted todo: {}", args.slug),
    }
    Ok(())
}
