// Rust guideline compliant 2026-02-21
//! Inbox view — list of unprocessed capture items.
//!
//! Items are sorted oldest-first. Each row shows:
//! `slug  body (truncated to 60 chars)  created_at`
//!
//! # Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `j` / `k` | Navigate |
//! | `n` | Add new capture item |
//! | `Enter` | Open process dialog for selected item |
//! | `D` | Hard-delete selected item (with confirmation) |
//! | `/` | Filter |
//!
//! This is a pure rendering function; no state is mutated here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::CaptureItem;
use crate::tui::app::App;
use crate::tui::components::table;
use crate::tui::types::Modal;

// DOCUMENTED-MAGIC: 60 characters is a comfortable truncation length for
// capture item bodies — long enough to be informative, short enough to keep
// rows from wrapping on typical 80-column terminals.
const BODY_TRUNCATE: usize = 60;

/// Renders the inbox view into `area`.
///
/// Applies the current filter from `app.captures.filter`. The highlighted row
/// is given by `app.captures.selected`. Modals are rendered on top.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let filter = app.captures.filter.to_lowercase();
    let visible: Vec<&CaptureItem> = app
        .captures
        .items
        .iter()
        .filter(|c| filter.is_empty() || c.body.to_lowercase().contains(&filter))
        .collect();

    if visible.is_empty() {
        let text = if filter.is_empty() {
            "  No inbox items. Press [n] to capture a thought."
        } else {
            "  No inbox items match the current filter."
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
            .map(|c| {
                let body = if c.body.len() > BODY_TRUNCATE {
                    format!("{}…", &c.body[..BODY_TRUNCATE])
                } else {
                    c.body.clone()
                };
                let created = c.created_at.format("%Y-%m-%d %H:%M").to_string();
                vec![c.slug.clone(), body, created]
            })
            .collect();

        let selected = app.captures.selected.min(visible.len().saturating_sub(1));
        table::render_table(
            frame,
            area,
            &["Slug", "Body", "Created"],
            rows,
            selected,
            &[
                Constraint::Min(22),
                Constraint::Fill(1),
                Constraint::Length(16),
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
