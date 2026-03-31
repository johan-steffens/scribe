// Rust guideline compliant 2026-02-21
//! Phase 4 placeholder view renderer.
//!
//! Views not yet implemented (Todos, Tracker, Inbox, Reminders) display a
//! centred "Coming in Phase 4" message. This module provides a single
//! [`render_placeholder`] function used for all four views.
//!
//! The real implementations will replace calls to this function in Phase 4.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;

/// Renders a centred "Coming in Phase 4" placeholder into `area`.
///
/// `view_name` is the display name shown in the heading, e.g. `"Todos"`.
pub fn render_placeholder(frame: &mut Frame, area: Rect, view_name: &str) {
    // Vertically centre the two-line message.
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    let heading = Paragraph::new(view_name)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let subtext = Paragraph::new("Coming in Phase 4")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(heading, vertical[1]);
    frame.render_widget(subtext, vertical[2]);
}
