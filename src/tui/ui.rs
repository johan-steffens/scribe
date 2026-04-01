// Rust guideline compliant 2026-02-21
//! Top-level layout composition and frame drawing.
//!
//! [`draw`] is the sole entry point called by the event loop on every tick.
//! It splits the terminal frame into three zones:
//!
//! 1. **Tab bar** (2 lines) — application name and navigation shortcuts.
//! 2. **Main content** (fills remaining space) — delegated to the active view.
//! 3. **Status bar** (2 lines) — timer info and key hints or last error.
//!
//! If `app.show_help` is true, a help overlay is rendered centred over the
//! main content area.
//!
//! This module is entirely pure — no state is mutated during a draw call.

use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::app::{App, View};
use crate::tui::components::status_bar;
use crate::tui::types::Modal;
use crate::tui::views::{dashboard, inbox, projects, reminders, tasks, todos, tracker};

// ── public entry point ─────────────────────────────────────────────────────

/// Draws the entire TUI frame.
///
/// Called on every event-loop iteration. Accepts an immutable reference to
/// `app` so the draw path is provably side-effect-free.
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Three vertical zones: tab bar / main / status bar.
    // DOCUMENTED-MAGIC: 2 lines for the tab bar and status bar each allows one
    // line for content and one line for a separator / hint on each edge.
    let zones = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .split(size);

    let tab_area = zones[0];
    let main_area = zones[1];
    let status_area = zones[2];

    render_tab_bar(frame, tab_area, app);
    render_main(frame, main_area, app);
    status_bar::render(frame, status_area, app);

    if app.show_help {
        render_help_overlay(frame, main_area);
    }
}

// ── private helpers ────────────────────────────────────────────────────────

/// Renders the two-line navigation tab bar.
fn render_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    // Split into two lines.
    let lines_area = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let active_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let inactive_style = Style::default().fg(Color::White);
    let label_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    // Helper: styles a navigation entry based on whether it is active.
    let nav_span = |view: View, label: &'static str| -> Span<'static> {
        if app.active_view == view {
            Span::styled(label, active_style)
        } else {
            Span::styled(label, inactive_style)
        }
    };

    let line1 = Line::from(vec![
        Span::styled("  Scribe  ", label_style),
        nav_span(View::Dashboard, "[D]ashboard "),
        nav_span(View::Projects, "[P]rojects "),
        nav_span(View::Tasks, "[T]asks "),
        nav_span(View::Todos, "[O]Todos"),
    ]);

    let line2 = Line::from(vec![
        Span::raw("            "),
        nav_span(View::Tracker, "T[R]acker   "),
        nav_span(View::Inbox, "[I]nbox    "),
        nav_span(View::Reminders, "[M]Reminders"),
    ]);

    frame.render_widget(Paragraph::new(line1), lines_area[0]);
    frame.render_widget(Paragraph::new(line2), lines_area[1]);
}

/// Delegates rendering to the active view.
fn render_main(frame: &mut Frame, area: Rect, app: &App) {
    match app.active_view {
        View::Dashboard => dashboard::render(frame, area, app),
        View::Projects => {
            projects::render(frame, area, app);
            // Render any active modal on top.
            render_modal(frame, area, app);
        }
        View::Tasks => {
            tasks::render(frame, area, app);
            render_modal(frame, area, app);
        }
        View::Todos => todos::render(frame, area, app),
        View::Tracker => tracker::render(frame, area, app),
        View::Inbox => inbox::render(frame, area, app),
        View::Reminders => reminders::render(frame, area, app),
    }
}

/// Renders any active modal overlay (form or confirm dialog).
///
/// Used for views (projects, tasks) that don't handle modal rendering
/// themselves.
fn render_modal(frame: &mut Frame, area: Rect, app: &App) {
    match &app.modal {
        Modal::Form(form, _) => form.render(frame, area),
        Modal::Confirm(dialog, _) => dialog.render(frame, area),
        Modal::None => {}
    }
}

/// Renders a centred help overlay popup.
fn render_help_overlay(frame: &mut Frame, area: Rect) {
    // The popup is centred and sized relative to the main area.
    // DOCUMENTED-MAGIC: 60% width and 70% height gives comfortable margins on
    // most terminal sizes while keeping all keybindings visible.
    let popup_area = centred_rect(60, 70, area);

    // Clear the background to avoid bleed-through from the view below.
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Keyboard Shortcuts ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);

    let bindings: &[(&str, &str)] = &[
        ("d", "Dashboard"),
        ("p", "Projects"),
        ("t", "Tasks"),
        ("o", "Todos"),
        ("r", "Tracker"),
        ("i", "Inbox"),
        ("m", "Reminders"),
        ("j / ↓", "Move selection down"),
        ("k / ↑", "Move selection up"),
        ("n", "New item"),
        ("e", "Edit selected"),
        ("D", "Delete/archive selected"),
        ("Space", "Toggle done / start-stop timer"),
        ("Enter", "Process inbox item"),
        ("v", "Move todo to different project"),
        ("/", "Enter filter mode"),
        ("Esc", "Clear filter / close modal / dismiss error"),
        ("Tab", "Next form field (in form)"),
        ("?", "Toggle this help"),
        ("q", "Quit"),
    ];

    let lines: Vec<Line<'_>> = bindings
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("  {key:<14}"), key_style),
                Span::styled(*desc, desc_style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    // Add a small margin inside the block.
    let content_area = inner.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    frame.render_widget(paragraph, content_area);
}

/// Computes a centred [`Rect`] with the given percentage dimensions.
///
/// `percent_x` and `percent_y` are values in `[0, 100]` representing the
/// fraction of `r` that the popup occupies.
fn centred_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
