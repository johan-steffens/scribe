// Rust guideline compliant 2026-02-21
//! Projects view — full-screen list of active projects.
//!
//! Each row shows:
//! `[status badge]  slug  name  [task count]`
//!
//! Live filter via `/` narrows by name or slug substring match.
//! The selected row is highlighted.
//!
//! This is a pure rendering function; no state is mutated here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::Project;
use crate::domain::project::ProjectStatus;
use crate::tui::app::App;
use crate::tui::components::table;

/// Renders the projects list into `area`.
///
/// Applies the current filter string from `app.projects.filter` to narrow the
/// visible rows. The highlighted row is given by `app.projects.selected`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let filter = app.projects.filter.to_lowercase();
    let visible: Vec<&Project> = app
        .projects
        .items
        .iter()
        .filter(|p| {
            if filter.is_empty() {
                true
            } else {
                p.slug.to_lowercase().contains(&filter) || p.name.to_lowercase().contains(&filter)
            }
        })
        .collect();

    if visible.is_empty() {
        let text = if filter.is_empty() {
            "  No projects found. Use `scribe project add` to create one."
        } else {
            "  No projects match the current filter."
        };
        let paragraph = Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, area);
        return;
    }

    // Count tasks per project from the already-loaded task list.
    let rows: Vec<Vec<String>> = visible
        .iter()
        .map(|p| {
            let task_count = app
                .tasks
                .items
                .iter()
                .filter(|t| t.project_id == p.id && t.archived_at.is_none())
                .count();
            vec![
                status_badge(p.status).to_owned(),
                p.slug.clone(),
                p.name.clone(),
                task_count.to_string(),
            ]
        })
        .collect();

    let selected = app.projects.selected.min(visible.len().saturating_sub(1));

    table::render_table(
        frame,
        area,
        &["Status", "Slug", "Name", "Tasks"],
        rows,
        selected,
        &[
            Constraint::Length(9),
            Constraint::Min(20),
            Constraint::Min(24),
            Constraint::Length(6),
        ],
    );
}

// ── utilities ──────────────────────────────────────────────────────────────

/// Returns a short status badge string for a project.
const fn status_badge(s: ProjectStatus) -> &'static str {
    match s {
        ProjectStatus::Active => "active   ",
        ProjectStatus::Paused => "paused   ",
        ProjectStatus::Completed => "completed",
    }
}
