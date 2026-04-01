// Rust guideline compliant 2026-02-21
//! Reminders view — list of active (non-archived, non-fired) reminders.
//!
//! Each row shows:
//! `slug  [project-slug]  [task-slug or —]  remind_at  message`
//!
//! # Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `j` / `k` | Navigate |
//! | `n` | Create new reminder |
//! | `e` | Edit selected reminder |
//! | `D` | Archive selected reminder (with confirmation) |
//! | `/` | Filter |
//!
//! This is a pure rendering function; no state is mutated here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::Reminder;
use crate::tui::app::App;
use crate::tui::components::table;
use crate::tui::types::Modal;

/// Renders the reminders view into `area`.
///
/// Applies the current filter from `app.reminders.filter`. The highlighted
/// row is given by `app.reminders.selected`. Modals are rendered on top.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let filter = app.reminders.filter.to_lowercase();
    let visible: Vec<&Reminder> = app
        .reminders
        .items
        .iter()
        .filter(|r| {
            filter.is_empty()
                || r.message
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&filter)
        })
        .collect();

    if visible.is_empty() {
        let text = if filter.is_empty() {
            "  No active reminders. Press [n] to create one."
        } else {
            "  No reminders match the current filter."
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
            .map(|r| {
                let project_slug = app
                    .projects
                    .items
                    .iter()
                    .find(|p| p.id == r.project_id)
                    .map_or_else(|| format!("#{}", r.project_id.0), |p| p.slug.clone());

                let task_slug = r.task_id.map_or("—".to_owned(), |t| format!("#{}", t.0));

                let remind_at = r.remind_at.format("%Y-%m-%d %H:%M").to_string();

                let message = {
                    let raw = r.message.as_deref().unwrap_or("");
                    if r.persistent {
                        format!("[P] {raw}")
                    } else {
                        raw.to_owned()
                    }
                };

                vec![r.slug.clone(), project_slug, task_slug, remind_at, message]
            })
            .collect();

        let selected = app.reminders.selected.min(visible.len().saturating_sub(1));
        table::render_table(
            frame,
            area,
            &["Slug", "Project", "Task", "Remind at", "Message"],
            rows,
            selected,
            &[
                Constraint::Min(24),
                Constraint::Min(14),
                Constraint::Min(10),
                Constraint::Length(16),
                Constraint::Fill(1),
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
