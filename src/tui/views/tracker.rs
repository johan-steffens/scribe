//! Tracker view — active timer display and time-entry history.
//!
//! The view is split into two sections:
//! - **Top** — Active timer (full-width, same as dashboard right panel).
//! - **Bottom** — Scrollable list of the last 50 non-archived time entries
//!   (most-recent first).
//!
//! Each entry row shows:
//! `slug  [project-slug]  [task-slug or —]  started  duration  note`
//!
//! # Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `j` / `k` | Navigate entry list |
//! | `Space` / `n` | Start timer (if idle) or stop timer (if running) |
//! | `e` | Edit note on selected entry |
//! | `D` | Archive selected entry (with confirmation) |
//!
//! This is a pure rendering function; no state is mutated here.

use chrono::Duration;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::domain::TimeEntry;
use crate::tui::app::App;
use crate::tui::components::table;
use crate::tui::types::Modal;

/// Renders the tracker view into `area`.
///
/// The top panel shows the active timer (if any). The bottom panel lists
/// recent time entries.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // DOCUMENTED-MAGIC: 4 rows for the timer panel gives one line per display
    // element (status, slug, elapsed, note) without wasting space.
    let panels = Layout::vertical([Constraint::Length(6), Constraint::Fill(1)]).split(area);

    render_active_timer(frame, panels[0], app);
    render_entry_list(frame, panels[1], app);

    // Render modal overlays on top.
    match &app.modal {
        Modal::Form(form, _) => form.render(frame, area),
        Modal::Confirm(dialog, _) => dialog.render(frame, area),
        Modal::None => {}
    }
}

// ── private helpers ────────────────────────────────────────────────────────

/// Renders the active timer panel.
fn render_active_timer(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Active Timer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some((entry, elapsed)) = &app.active_timer {
        render_timer_running(frame, inner, entry, *elapsed);
    } else {
        let text = Paragraph::new(vec![
            Line::from(Span::styled(
                " No active timer",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                " Press [Space] or [n] to start a timer",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        frame.render_widget(text, inner);
    }
}

/// Renders the details of a running timer.
fn render_timer_running(frame: &mut Frame, area: Rect, entry: &TimeEntry, elapsed: Duration) {
    let hours = elapsed.num_hours();
    let mins = elapsed.num_minutes() % 60;
    let secs = elapsed.num_seconds() % 60;

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            " ● Running",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(" Entry:   ", Style::default().fg(Color::DarkGray)),
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
        Line::from(vec![
            Span::styled(" Project: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                entry.project_id.0.to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    if let Some(ref note) = entry.note {
        lines.push(Line::from(vec![
            Span::styled(" Note:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(note.as_str(), Style::default().fg(Color::White)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Renders the scrollable entry list.
fn render_entry_list(frame: &mut Frame, area: Rect, app: &App) {
    if app.entries.items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  No time entries yet.",
                Style::default().fg(Color::DarkGray),
            ))),
            area,
        );
        return;
    }

    let rows: Vec<Vec<String>> = app
        .entries
        .items
        .iter()
        .map(|e| {
            let project_slug = app
                .projects
                .items
                .iter()
                .find(|p| p.id == e.project_id)
                .map_or_else(|| format!("#{}", e.project_id.0), |p| p.slug.clone());

            let task_slug = e.task_id.map_or("—".to_owned(), |t| format!("#{}", t.0));

            let started = e.started_at.format("%m-%d %H:%M").to_string();

            let duration = match e.ended_at {
                Some(ended) => {
                    let dur = ended - e.started_at;
                    let h = dur.num_hours();
                    let m = dur.num_minutes() % 60;
                    format!("{h}h{m}m")
                }
                None => "running".to_owned(),
            };

            let note = e.note.as_deref().unwrap_or("").to_owned();

            vec![
                e.slug.clone(),
                project_slug,
                task_slug,
                started,
                duration,
                note,
            ]
        })
        .collect();

    let selected = app
        .entries
        .selected
        .min(app.entries.items.len().saturating_sub(1));

    table::render_table(
        frame,
        area,
        &["Slug", "Project", "Task", "Started", "Duration", "Note"],
        rows,
        selected,
        &[
            Constraint::Min(28),
            Constraint::Min(14),
            Constraint::Min(10),
            Constraint::Length(11),
            Constraint::Length(8),
            Constraint::Fill(1),
        ],
    );
}
