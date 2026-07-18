//! CLI subcommands for managing notes (`scribe note …`).
//!
//! Notes are long-form Markdown documents with bi-directional `[[slug]]`
//! linking support. They can be created, edited, listed, and searched.
//!
//! All subcommands support `--output json` for machine-readable output.

use clap::{Args, Subcommand};
use serde_json::json;

use crate::cli::project::OutputFormat;
use crate::ops::NotesOps;

/// Arguments for the `scribe note` subcommand group.
#[derive(Debug, Args)]
pub struct NoteCommand {
    /// Note subcommand.
    #[command(subcommand)]
    pub subcommand: NoteSubcommand,
}

/// All `scribe note` subcommands.
#[derive(Debug, Subcommand)]
pub enum NoteSubcommand {
    /// Create a new note and open it in $EDITOR.
    Add(NoteAdd),
    /// Create a new note with inline content (no editor).
    Create(NoteCreate),
    /// Edit an existing note in $EDITOR.
    Edit(NoteEdit),
    /// List all notes.
    List(NoteList),
    /// Show a note by slug.
    Show(NoteShow),
    /// Delete a note.
    Delete(NoteDelete),
}

/// Arguments for `scribe note add`.
#[derive(Debug, Args)]
pub struct NoteAdd {
    /// Unique kebab-case slug for the note.
    pub slug: String,
    /// Title for the note.
    #[arg(long)]
    pub title: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe note create`.
#[derive(Debug, Args)]
pub struct NoteCreate {
    /// Title for the note (slug is auto-generated).
    pub title: String,
    /// Markdown content for the note.
    #[arg(long)]
    pub content: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe note edit`.
#[derive(Debug, Args)]
pub struct NoteEdit {
    /// Note slug to edit.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe note list`.
#[derive(Debug, Args)]
pub struct NoteList {
    /// Search notes by full-text query.
    #[arg(long)]
    pub search: Option<String>,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe note show`.
#[derive(Debug, Args)]
pub struct NoteShow {
    /// Note slug to show.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Arguments for `scribe note delete`.
#[derive(Debug, Args)]
pub struct NoteDelete {
    /// Note slug to delete.
    pub slug: String,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,
}

/// Executes a `note` subcommand against the given ops layer.
///
/// Prints results to stdout and errors to stderr. Returns `Ok(())` on success.
///
/// # Errors
///
/// Returns an error if the operation fails (e.g. note not found, DB error).
pub fn run(cmd: &NoteCommand, ops: &NotesOps) -> anyhow::Result<()> {
    match &cmd.subcommand {
        NoteSubcommand::Add(args) => handle_add(args, ops),
        NoteSubcommand::Create(args) => handle_create(args, ops),
        NoteSubcommand::Edit(args) => handle_edit(args, ops),
        NoteSubcommand::List(args) => handle_list(args, ops),
        NoteSubcommand::Show(args) => handle_show(args, ops),
        NoteSubcommand::Delete(args) => handle_delete(args, ops),
    }
}

fn handle_add(args: &NoteAdd, ops: &NotesOps) -> anyhow::Result<()> {
    let note = ops.create_and_edit(&args.title, &args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&note)?),
        OutputFormat::Text => {
            println!("Created and editing: {} ({})", note.title, note.slug);
            println!("Content saved to: /tmp/scribe-note-{}.md", note.slug);
        }
    }
    Ok(())
}

fn handle_create(args: &NoteCreate, ops: &NotesOps) -> anyhow::Result<()> {
    let content = args.content.as_deref().unwrap_or("");
    let note = ops.write_note(&args.title, content)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&note)?),
        OutputFormat::Text => println!("Created note: {} ({})", note.title, note.slug),
    }
    Ok(())
}

fn handle_edit(args: &NoteEdit, ops: &NotesOps) -> anyhow::Result<()> {
    let note = ops.edit_note(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&note)?),
        OutputFormat::Text => {
            println!("Edited note: {} ({})", note.title, note.slug);
        }
    }
    Ok(())
}

fn handle_list(args: &NoteList, ops: &NotesOps) -> anyhow::Result<()> {
    let notes = if let Some(query) = &args.search {
        ops.search_notes(query)?
    } else {
        ops.list()?
    };
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&notes)?),
        OutputFormat::Text => {
            if notes.is_empty() {
                println!("No notes found.");
            } else {
                for n in &notes {
                    let preview: String = n.content.chars().take(50).collect();
                    println!(
                        "{:<40} {} [{}{}]",
                        n.slug,
                        n.title,
                        preview,
                        if preview.len() == 50 { "..." } else { "" }
                    );
                }
            }
        }
    }
    Ok(())
}

fn handle_show(args: &NoteShow, ops: &NotesOps) -> anyhow::Result<()> {
    let note = ops
        .get(&args.slug)?
        .ok_or_else(|| anyhow::anyhow!("note '{}' not found", args.slug))?;
    match args.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&note)?),
        OutputFormat::Text => {
            println!("slug:      {}", note.slug);
            println!("title:     {}", note.title);
            println!("--- content ---");
            println!("{}", note.content);
            println!("--- end ---");
            println!(
                "created:   {}",
                note.created_at.format("%Y-%m-%d %H:%M UTC")
            );
            println!(
                "updated:   {}",
                note.updated_at.format("%Y-%m-%d %H:%M UTC")
            );
        }
    }
    Ok(())
}

fn handle_delete(args: &NoteDelete, ops: &NotesOps) -> anyhow::Result<()> {
    ops.delete_note(&args.slug)?;
    match args.output {
        OutputFormat::Json => println!("{}", json!({ "deleted": args.slug })),
        OutputFormat::Text => println!("Deleted note: {}", args.slug),
    }
    Ok(())
}
