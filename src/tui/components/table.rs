//! Reusable stateful table widget wrapping [`ratatui::widgets::Table`].
//!
//! [`render_table`] is a pure function that accepts headers, rows, a selected
//! index, and column constraints, and renders a styled table inside the given
//! area. The selected row is highlighted with a contrasting background.
//!
//! # Design
//!
//! The component deliberately has no internal state — all state lives in
//! [`crate::tui::types::ViewState`]. This keeps the rendering path pure and
//! allocation-efficient.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

/// Renders a stateful table into `area`.
///
/// `headers` is a slice of column header labels. `rows` is a vector of
/// string-cell rows; each inner `Vec<String>` must have the same length as
/// `headers`. `selected` is the zero-based index of the highlighted row.
/// `constraints` controls column widths and must match the number of headers.
///
/// # Examples
///
/// ```ignore
/// render_table(
///     frame,
///     area,
///     &["Name", "Status"],
///     vec![vec!["foo".into(), "active".into()]],
///     0,
///     &[Constraint::Percentage(60), Constraint::Percentage(40)],
/// );
/// ```
pub fn render_table(
    frame: &mut Frame,
    area: Rect,
    headers: &[&str],
    rows: Vec<Vec<String>>,
    selected: usize,
    constraints: &[Constraint],
) {
    let header_cells: Vec<Cell<'_>> = headers
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();

    let header_row = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD))
        // Reserve one blank line after the header.
        .height(1);

    let data_rows: Vec<Row<'_>> = rows
        .into_iter()
        .map(|cells| {
            let row_cells: Vec<Cell<'_>> = cells.into_iter().map(Cell::from).collect();
            Row::new(row_cells).height(1)
        })
        .collect();

    let table = Table::new(data_rows, constraints)
        .header(header_row)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut state);
}
