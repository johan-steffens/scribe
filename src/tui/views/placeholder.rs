//! Placeholder view renderer for unimplemented views.
//!
//! This module is retained for potential Phase 5+ views that are not yet
//! implemented. The four Phase 4 views (Todos, Tracker, Inbox, Reminders)
//! now have real implementations and no longer use this placeholder.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;

/// Renders a centred placeholder message into `area`.
///
/// `view_name` is the display name shown in the heading. Retained for any
/// future Phase 5+ views before they are fully implemented.
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

    let subtext = Paragraph::new("Coming in a future phase")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(heading, vertical[1]);
    frame.render_widget(subtext, vertical[2]);
}
