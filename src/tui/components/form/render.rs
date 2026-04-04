//! Rendering utilities for the form widget.
//!
//! These private helpers are split out of `form.rs` to keep file sizes
//! manageable. All functions operate on ratatui primitives.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::FormField;

/// Builds the vertical layout rows for `n` form fields.
///
/// Each field occupies 3 rows: label, input, gap.
/// Two extra rows are added at the bottom: a fill padding and a hint line.
pub(super) fn build_field_layout(area: Rect, n: usize) -> std::rc::Rc<[Rect]> {
    // DOCUMENTED-MAGIC: each field takes 3 rows: label line, input line,
    // and one blank separator line.
    let mut constraints: Vec<Constraint> = Vec::with_capacity(n * 3 + 2);
    for _ in 0..n {
        constraints.push(Constraint::Length(1)); // label
        constraints.push(Constraint::Length(1)); // input
        constraints.push(Constraint::Length(1)); // gap
    }
    constraints.push(Constraint::Fill(1)); // bottom padding
    constraints.push(Constraint::Length(1)); // hint line
    Layout::vertical(constraints).split(area)
}

/// Renders a single form field (label + input widget) into the appropriate rows.
pub(super) fn render_field(
    frame: &mut Frame,
    field: &FormField,
    is_focused: bool,
    rows: &[Rect],
    field_idx: usize,
) {
    let label_style = if is_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let label_row = rows[field_idx * 3];
    let input_row = rows[field_idx * 3 + 1];

    frame.render_widget(Paragraph::new(field.label()).style(label_style), label_row);

    match field {
        FormField::Text {
            value,
            placeholder,
            cursor,
            ..
        } => render_text_field(frame, value, placeholder, *cursor, is_focused, input_row),
        FormField::DateTime {
            value,
            error,
            cursor,
            ..
        } => render_datetime_field(
            frame,
            value,
            error.as_deref(),
            *cursor,
            is_focused,
            input_row,
            rows[field_idx * 3 + 2],
        ),
        FormField::Select {
            options, selected, ..
        } => render_select_field(frame, options, *selected, is_focused, input_row),
    }
}

/// Renders a text input field.
pub(super) fn render_text_field(
    frame: &mut Frame,
    value: &str,
    placeholder: &str,
    cursor: usize,
    is_focused: bool,
    area: Rect,
) {
    let input_style = if is_focused {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };
    let line = if is_focused {
        Paragraph::new(build_cursor_line(value, cursor)).style(input_style)
    } else if value.is_empty() {
        Paragraph::new(Span::styled(
            placeholder,
            Style::default().fg(Color::DarkGray),
        ))
        .style(input_style)
    } else {
        Paragraph::new(Span::styled(value, Style::default().fg(Color::White))).style(input_style)
    };
    frame.render_widget(line, area);
}

/// Renders a datetime text input field with optional validation error.
pub(super) fn render_datetime_field(
    frame: &mut Frame,
    value: &str,
    error: Option<&str>,
    cursor: usize,
    is_focused: bool,
    input_area: Rect,
    error_area: Rect,
) {
    let input_style = if is_focused {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };
    let line = if is_focused {
        Paragraph::new(build_cursor_line(value, cursor)).style(input_style)
    } else if value.is_empty() {
        Paragraph::new(Span::styled(
            "YYYY-MM-DD HH:MM",
            Style::default().fg(Color::DarkGray),
        ))
        .style(input_style)
    } else {
        Paragraph::new(Span::styled(value, Style::default().fg(Color::White))).style(input_style)
    };
    frame.render_widget(line, input_area);

    if let Some(err) = error {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("  ⚠ {err}"),
                Style::default().fg(Color::Red),
            )),
            error_area,
        );
    }
}

/// Renders a select (dropdown) field.
pub(super) fn render_select_field(
    frame: &mut Frame,
    options: &[String],
    selected: usize,
    is_focused: bool,
    area: Rect,
) {
    if is_focused {
        // Show a mini list; height is capped at 4 visible items.
        // DOCUMENTED-MAGIC: 4 items fits within typical popup heights
        // while remaining scannable without scrolling.
        let visible = options.len().min(4);
        let start = selected.saturating_sub(visible.saturating_sub(1));
        let end = options.len().min(start + visible);
        let items: Vec<ListItem<'_>> = options[start..end]
            .iter()
            .enumerate()
            .map(|(j, opt)| {
                let actual = j + start;
                if actual == selected {
                    ListItem::new(Span::styled(
                        format!("> {opt}"),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    ListItem::new(Span::styled(
                        format!("  {opt}"),
                        Style::default().fg(Color::Gray),
                    ))
                }
            })
            .collect();
        let list = List::new(items);
        let mut list_state =
            ListState::default().with_selected(Some(selected.saturating_sub(start)));
        frame.render_stateful_widget(list, area, &mut list_state);
    } else {
        let selected_label = options.get(selected).map_or("—", String::as_str);
        frame.render_widget(
            Paragraph::new(Span::styled(
                selected_label,
                Style::default().fg(Color::Gray),
            )),
            area,
        );
    }
}

/// Builds a `Line` that shows the cursor as a highlighted block character.
pub(super) fn build_cursor_line(value: &str, cursor: usize) -> Line<'static> {
    let before = value[..cursor].to_owned();
    let after = value[cursor..].to_owned();

    let cursor_char = if after.is_empty() {
        " ".to_owned()
    } else {
        after
            .chars()
            .next()
            .map_or(" ".to_owned(), |c| c.to_string())
    };

    let after_cursor = if after.is_empty() {
        String::new()
    } else {
        after[cursor_char.len()..].to_owned()
    };

    Line::from(vec![
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled(
            cursor_char,
            Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(after_cursor, Style::default().fg(Color::White)),
    ])
}

/// Computes a centred popup [`Rect`] sized for the given number of fields.
pub(super) fn centred_form_rect(area: Rect, field_count: usize) -> Rect {
    // Each field: 1 label + 1 input + 1 gap = 3 rows; plus borders (2) and hint (1).
    // DOCUMENTED-MAGIC: cap at 24 rows so the popup never exceeds the screen.
    let height = u16::try_from(field_count * 3 + 4)
        .unwrap_or(u16::MAX)
        .min(24)
        .min(area.height);
    let top = area.height.saturating_sub(height) / 3;

    // DOCUMENTED-MAGIC: 70 % width gives comfortable space for most field values.
    let width = (area.width * 70 / 100).max(40).min(area.width);
    let left = (area.width.saturating_sub(width)) / 2;

    Rect {
        x: area.x + left,
        y: area.y + top,
        width,
        height,
    }
}

/// Handles a key press on a text or datetime input field.
///
/// Modifies `value` and `cursor` in place.
pub(super) fn handle_text_key(value: &mut String, cursor: &mut usize, code: KeyCode) {
    match code {
        KeyCode::Char(c) => {
            value.insert(*cursor, c);
            *cursor += c.len_utf8();
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                let new_cursor = value[..*cursor]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(i, _)| i);
                value.drain(new_cursor..*cursor);
                *cursor = new_cursor;
            }
        }
        KeyCode::Left => {
            if *cursor > 0 {
                *cursor = value[..*cursor]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(i, _)| i);
            }
        }
        KeyCode::Right => {
            if *cursor < value.len() {
                let next = value[*cursor..]
                    .char_indices()
                    .nth(1)
                    .map_or(value.len(), |(i, _)| *cursor + i);
                *cursor = next;
            }
        }
        KeyCode::Home => {
            *cursor = 0;
        }
        KeyCode::End => {
            *cursor = value.len();
        }
        _ => {}
    }
}
