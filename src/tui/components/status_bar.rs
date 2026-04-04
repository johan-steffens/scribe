//! Status bar component rendered at the bottom of every TUI frame.
//!
//! The status bar occupies two lines:
//! - **Line 1** — active timer info or "No active timer".
//! - **Line 2** — key hint text, or the last error message in red when set.
//!
//! This is a pure rendering function; it holds no state.

use chrono::Duration;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::TimeEntry;
use crate::tui::app::{App, InputMode};

/// Renders the two-line status bar into `area`.
///
/// Line 1 shows timer information (or "No active timer"). Line 2 shows key
/// hints, or `app.last_error` in red when set.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Split the area into two 1-line rows.
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    render_timer_line(frame, rows[0], app.active_timer.as_ref());
    render_hint_line(frame, rows[1], app);
}

// ── private helpers ────────────────────────────────────────────────────────

/// Formats and renders the timer info line.
fn render_timer_line(frame: &mut Frame, area: Rect, active_timer: Option<&(TimeEntry, Duration)>) {
    let text = match active_timer {
        Some((entry, elapsed)) => {
            let hours = elapsed.num_hours();
            // `num_minutes()` returns total minutes; we want the remainder after hours.
            let mins = elapsed.num_minutes() % 60;
            let secs = elapsed.num_seconds() % 60;
            let note_suffix = entry
                .note
                .as_deref()
                .map_or_else(String::new, |n| format!(" — {n}"));
            format!(
                "  Timer: {}  {hours}h {mins}m {secs}s{note_suffix}",
                entry.slug
            )
        }
        None => "  No active timer".to_owned(),
    };

    let style = if active_timer.is_some() {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let paragraph = Paragraph::new(text).style(style);
    frame.render_widget(paragraph, area);
}

/// Renders key hints or the last error message.
fn render_hint_line(frame: &mut Frame, area: Rect, app: &App) {
    let paragraph = if let Some(ref err) = app.last_error {
        let line = Line::from(vec![
            Span::styled(
                "  Error: ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(err.as_str(), Style::default().fg(Color::Red)),
            Span::styled("  [Esc] dismiss", Style::default().fg(Color::DarkGray)),
        ]);
        Paragraph::new(line)
    } else if app.input_mode == InputMode::Filter {
        let filter_text = match app.active_view {
            crate::tui::app::View::Projects => app.projects.filter.as_str(),
            crate::tui::app::View::Tasks | crate::tui::app::View::Dashboard => {
                app.tasks.filter.as_str()
            }
            crate::tui::app::View::Todos => app.todos.filter.as_str(),
            crate::tui::app::View::Tracker => app.entries.filter.as_str(),
            crate::tui::app::View::Inbox => app.captures.filter.as_str(),
            crate::tui::app::View::Reminders => app.reminders.filter.as_str(),
        };
        let line = Line::from(vec![
            Span::styled(
                "  Filter: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(filter_text, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::Yellow)),
            Span::styled(
                "  [Esc] clear  [Enter] confirm",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        Paragraph::new(line)
    } else {
        let line = Line::from(vec![Span::styled(
            "  [?] Help  [q] Quit  [d/p/t/o/r/i/m] Views  [j/k] Navigate  [/] Filter",
            Style::default().fg(Color::DarkGray),
        )]);
        Paragraph::new(line)
    };

    frame.render_widget(paragraph, area);
}
