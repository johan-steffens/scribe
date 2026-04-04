//! Dashboard view — today's tasks and active timer panel.
//!
//! The dashboard is split into two side-by-side panels:
//! - **Left** — "Today's Tasks": active tasks with a due date of today or
//!   earlier, sorted urgent-first.
//! - **Right** — "Active Timer": running timer details, or a placeholder hint
//!   if no timer is active.
//!
//! This is a pure rendering function; no state is mutated here.

use chrono::{Duration, Local};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::domain::task::TaskPriority;
use crate::domain::task::TaskStatus;
use crate::domain::{Task, TimeEntry};
use crate::tui::app::App;
use crate::tui::components::table;

/// Renders the dashboard into `area`.
///
/// The area is split horizontally into a left task panel and a right timer
/// panel. Data is read from `app` with no DB calls.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let panels =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);

    render_today_tasks(frame, panels[0], app);
    render_active_timer(frame, panels[1], app);
}

// ── private helpers ────────────────────────────────────────────────────────

/// Renders the "Today's Tasks" left panel.
fn render_today_tasks(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Today's Tasks ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let today = Local::now().date_naive();
    let mut due_tasks: Vec<&Task> = app
        .tasks
        .items
        .iter()
        .filter(|t| {
            // Show non-archived, non-done, non-cancelled tasks due today or overdue.
            t.archived_at.is_none()
                && t.status != TaskStatus::Done
                && t.status != TaskStatus::Cancelled
                && t.due_date.is_some_and(|d| d <= today)
        })
        .collect();

    // Sort: urgent first, then high, medium, low.
    due_tasks.sort_by_key(|t| priority_sort_key(t.priority));

    if due_tasks.is_empty() {
        let msg =
            Paragraph::new("  No tasks due today.").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(msg, inner);
        return;
    }

    let headers = ["Pri", "Title", "Project"];
    let rows: Vec<Vec<String>> = due_tasks
        .iter()
        .map(|t| {
            vec![
                priority_badge(t.priority).to_owned(),
                t.title.clone(),
                project_slug_for_task(t, app),
            ]
        })
        .collect();

    let selected = app.tasks.selected.min(due_tasks.len().saturating_sub(1));

    table::render_table(
        frame,
        inner,
        &headers,
        rows,
        selected,
        &[
            Constraint::Length(4),
            Constraint::Min(20),
            Constraint::Min(12),
        ],
    );
}

/// Renders the "Active Timer" right panel.
fn render_active_timer(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Active Timer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    match &app.active_timer {
        Some((entry, elapsed)) => render_timer_details(frame, inner, entry, *elapsed),
        None => render_no_timer(frame, inner),
    }
}

/// Renders details for a running timer.
fn render_timer_details(frame: &mut Frame, area: Rect, entry: &TimeEntry, elapsed: Duration) {
    let hours = elapsed.num_hours();
    let mins = elapsed.num_minutes() % 60;
    let secs = elapsed.num_seconds() % 60;

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            " Running",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Entry: ", Style::default().fg(Color::DarkGray)),
            Span::styled(entry.slug.as_str(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" Elapsed: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{hours}h {mins}m {secs}s"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if let Some(ref note) = entry.note {
        lines.push(Line::from(vec![
            Span::styled(" Note: ", Style::default().fg(Color::DarkGray)),
            Span::styled(note.as_str(), Style::default().fg(Color::White)),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Renders a placeholder when no timer is active.
fn render_no_timer(frame: &mut Frame, area: Rect) {
    let lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            " No active timer",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            " Press [Space] to start a timer",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            " (Phase 4)",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
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

/// Returns a sort key where lower numbers sort first (urgent = 0).
const fn priority_sort_key(p: TaskPriority) -> u8 {
    match p {
        TaskPriority::Urgent => 0,
        TaskPriority::High => 1,
        TaskPriority::Medium => 2,
        TaskPriority::Low => 3,
    }
}

/// Looks up the project slug that owns `task` by scanning `app.projects`.
///
/// Falls back to the raw project ID if not found (should never happen in
/// practice since `refresh()` loads all active projects).
fn project_slug_for_task(task: &Task, app: &App) -> String {
    app.projects
        .items
        .iter()
        .find(|p| p.id == task.project_id)
        .map_or_else(|| format!("#{}", task.project_id.0), |p| p.slug.clone())
}
