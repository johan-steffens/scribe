// Rust guideline compliant 2026-02-21
//! Tasks view — full-screen list of active tasks.
//!
//! Each row shows:
//! `[priority badge]  [status badge]  title  [project-slug]  [due date]`
//!
//! Live filter via `/` narrows by title or project-slug substring match.
//! The selected row is highlighted.
//!
//! This is a pure rendering function; no state is mutated here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::Task;
use crate::domain::task::{TaskPriority, TaskStatus};
use crate::tui::app::App;
use crate::tui::components::table;

/// Renders the tasks list into `area`.
///
/// Applies the current filter string from `app.tasks.filter`. The highlighted
/// row index is `app.tasks.selected`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let filter = app.tasks.filter.to_lowercase();
    let visible: Vec<&Task> = app
        .tasks
        .items
        .iter()
        .filter(|t| {
            if filter.is_empty() {
                true
            } else {
                let project_slug = app
                    .projects
                    .items
                    .iter()
                    .find(|p| p.id == t.project_id)
                    .map_or("", |p| p.slug.as_str());
                t.title.to_lowercase().contains(&filter)
                    || project_slug.to_lowercase().contains(&filter)
            }
        })
        .collect();

    if visible.is_empty() {
        let text = if filter.is_empty() {
            "  No tasks found. Use `scribe task add` to create one."
        } else {
            "  No tasks match the current filter."
        };
        let paragraph = Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, area);
        return;
    }

    let rows: Vec<Vec<String>> = visible
        .iter()
        .map(|t| {
            let project_slug = app
                .projects
                .items
                .iter()
                .find(|p| p.id == t.project_id)
                .map_or_else(|| format!("#{}", t.project_id.0), |p| p.slug.clone());

            let due = t
                .due_date
                .map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string());

            vec![
                priority_badge(t.priority).to_owned(),
                status_badge(t.status).to_owned(),
                t.title.clone(),
                project_slug,
                due,
            ]
        })
        .collect();

    let selected = app.tasks.selected.min(visible.len().saturating_sub(1));

    table::render_table(
        frame,
        area,
        &["Pri", "Status", "Title", "Project", "Due"],
        rows,
        selected,
        &[
            Constraint::Length(4),
            Constraint::Length(11),
            Constraint::Min(24),
            Constraint::Min(16),
            Constraint::Length(10),
        ],
    );
}

// ── utilities ──────────────────────────────────────────────────────────────

/// Returns a short priority badge string for display.
const fn priority_badge(p: TaskPriority) -> &'static str {
    match p {
        TaskPriority::Urgent => "URGN",
        TaskPriority::High => "HIGH",
        TaskPriority::Medium => "MED ",
        TaskPriority::Low => "LOW ",
    }
}

/// Returns a short status badge string for display.
const fn status_badge(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Todo => "todo      ",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Done => "done      ",
        TaskStatus::Cancelled => "cancelled ",
    }
}
