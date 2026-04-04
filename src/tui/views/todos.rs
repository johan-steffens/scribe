//! Todos view — full-screen list of active todos.
//!
//! Each row shows:
//! `[✓/○]  title  [project-slug]`
//!
//! Live filter via `/` narrows by title substring match. The selected row
//! is highlighted.
//!
//! # Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `j` / `k` | Navigate |
//! | `n` | New todo |
//! | `e` | Edit selected |
//! | `D` | Archive selected (with confirmation) |
//! | `Space` | Toggle done/undone |
//! | `v` | Move selected to different project |
//! | `/` | Filter |
//!
//! This is a pure rendering function; no state is mutated here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::Todo;
use crate::tui::app::App;
use crate::tui::components::table;
use crate::tui::types::Modal;

/// Renders the todos list into `area`.
///
/// Applies the current filter from `app.todos.filter`. The highlighted row
/// is given by `app.todos.selected`. If a modal is active it is rendered on
/// top of the list.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let filter = app.todos.filter.to_lowercase();
    let visible: Vec<&Todo> = app
        .todos
        .items
        .iter()
        .filter(|t| filter.is_empty() || t.title.to_lowercase().contains(&filter))
        .collect();

    if visible.is_empty() {
        let text = if filter.is_empty() {
            "  No todos found. Press [n] to create one."
        } else {
            "  No todos match the current filter."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                Style::default().fg(Color::DarkGray),
            ))),
            area,
        );
    } else {
        let rows: Vec<Vec<String>> = visible
            .iter()
            .map(|t| {
                let check = if t.done { "✓" } else { "○" };
                let project_slug = app
                    .projects
                    .items
                    .iter()
                    .find(|p| p.id == t.project_id)
                    .map_or_else(|| format!("#{}", t.project_id.0), |p| p.slug.clone());
                vec![check.to_owned(), t.title.clone(), project_slug]
            })
            .collect();

        let selected = app.todos.selected.min(visible.len().saturating_sub(1));
        table::render_table(
            frame,
            area,
            &["", "Title", "Project"],
            rows,
            selected,
            &[
                Constraint::Length(2),
                Constraint::Min(28),
                Constraint::Min(16),
            ],
        );
    }

    // Render modal overlays on top.
    match &app.modal {
        Modal::Form(form, _) => form.render(frame, area),
        Modal::Confirm(dialog, _) => dialog.render(frame, area),
        Modal::None => {}
    }
}
