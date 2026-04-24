//! Notes view — split-pane list and markdown preview.
//!
//! The left pane shows a searchable list of all notes. The right pane renders
//! the selected note's markdown content and displays inbound backlinks below
//! the preview.
//!
//! Keyboard shortcuts in this view:
//! - `e` — open the selected note in `$EDITOR`
//! - `g` — jump to a linked task (if the selected note links to one)
//! - `/` — enter filter mode to search by slug or title
//!
//! This is a pure rendering function; no state is mutated here.

use pulldown_cmark::{Parser, html};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::domain::Note;
use crate::tui::app::App;

/// Column constraints for the notes list.
const LIST_WIDTH: u16 = 30;

/// Renders the notes split-pane view into `area`.
///
/// Left pane: searchable note list. Right pane: markdown preview + backlinks.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let visible = build_visible_notes(app);

    if visible.is_empty() {
        render_empty_state(frame, area, app);
        return;
    }

    // Split into list (left) and preview (right) panes.
    let split =
        Layout::horizontal([Constraint::Length(LIST_WIDTH), Constraint::Fill(1)]).split(area);

    let list_area = split[0];
    let preview_area = split[1];

    render_note_list(frame, list_area, app, &visible);
    render_note_preview(frame, preview_area, app);
}

/// Builds the filtered list of visible notes.
fn build_visible_notes(app: &App) -> Vec<&Note> {
    let filter = app.notes.filter.to_lowercase();
    app.notes
        .items
        .iter()
        .filter(|n| {
            if filter.is_empty() {
                true
            } else {
                n.slug.to_lowercase().contains(&filter) || n.title.to_lowercase().contains(&filter)
            }
        })
        .collect()
}

/// Renders the note list in the left pane.
fn render_note_list(frame: &mut Frame, area: Rect, app: &App, visible: &[&Note]) {
    let block = Block::default()
        .title(" Notes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let list_area = block.inner(area);
    frame.render_widget(block, area);

    if visible.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "  No notes found.",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, list_area);
        return;
    }

    let selected = app.notes.selected.min(visible.len().saturating_sub(1));

    let items: Vec<ListItem<'_>> = visible
        .iter()
        .map(|note| {
            let slug = &note.slug;
            let title = &note.title;
            let line = if title.is_empty() {
                Line::from(Span::raw(slug.clone()))
            } else {
                Line::from(vec![
                    Span::styled(format!("{slug} "), Style::default().fg(Color::DarkGray)),
                    Span::raw(title),
                ])
            };
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(Block::default()).highlight_style(
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, list_area, &mut state);
}

/// Renders the markdown preview and backlinks in the right pane.
fn render_note_preview(frame: &mut Frame, area: Rect, app: &App) {
    let visible = build_visible_notes(app);
    let Some(selected) = visible.get(app.notes.selected) else {
        return;
    };

    let note = *selected;

    // Split vertically: markdown preview (top) and backlinks (bottom).
    let split = Layout::vertical([Constraint::Fill(1), Constraint::Length(5)]).split(area);

    let preview_area = split[0];
    let links_area = split[1];

    // Render markdown content.
    render_markdown(frame, preview_area, note);

    // Render backlinks section.
    render_backlinks(frame, links_area, app, note);
}

/// Renders the markdown content of a note using `pulldown-cmark`.
fn render_markdown(frame: &mut Frame, area: Rect, note: &Note) {
    let block = Block::default()
        .title(format!(" {} ", note.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let content_area = block.inner(area);
    frame.render_widget(block, area);

    if note.content.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "  (empty note)",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, content_area);
        return;
    }

    // Parse markdown and convert to HTML, then strip HTML tags for plain text rendering.
    // This is a simplified approach - a full implementation would map markdown
    // elements to ratatui styled spans.
    let parser = Parser::new(&note.content);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    // Strip HTML tags for plain text display
    let plain_text = strip_html_tags(&html_output);

    let paragraph = Paragraph::new(Line::from(Span::raw(plain_text)));
    frame.render_widget(paragraph, content_area);
}

/// Strips HTML tags from a string for plain text display.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_entity = false;
    let mut entity_buf = String::new();

    for ch in html.chars() {
        if ch == '&' {
            in_entity = true;
            entity_buf.clear();
        } else if in_entity {
            entity_buf.push(ch);
            if ch == ';' {
                // Convert common HTML entities to their characters
                let entity = &entity_buf[..entity_buf.len() - 1];
                let ch_to_push = match entity {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ => {
                        // For other entities, just show the raw form
                        result.push('&');
                        result.push_str(entity);
                        result.push(';');
                        ' '
                    }
                };
                result.push(ch_to_push);
                in_entity = false;
                entity_buf.clear();
            }
        } else if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }

    // Clean up extra whitespace
    let mut cleaned = String::new();
    let mut last_was_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !last_was_space && !cleaned.is_empty() {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(ch);
            last_was_space = false;
        }
    }
    cleaned.trim().to_string()
}

/// Renders the backlinks section showing inbound links to this note.
fn render_backlinks(frame: &mut Frame, area: Rect, app: &App, _note: &Note) {
    let block = Block::default()
        .title(" Mentions / Linked To ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let links_area = block.inner(area);
    frame.render_widget(block, area);

    if app.note_links.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "  No backlinks.",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, links_area);
        return;
    }

    // Show each backlink with a hint about the 'g' key to jump.
    let items: Vec<ListItem<'_>> = app
        .note_links
        .iter()
        .map(|link| {
            let line = Line::from(vec![
                Span::styled("[g] ", Style::default().fg(Color::Yellow)),
                Span::raw(link.source_slug.clone()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(Block::default());
    frame.render_widget(list, links_area);
}

/// Renders an empty state when no notes exist or none match the filter.
fn render_empty_state(frame: &mut Frame, area: Rect, app: &App) {
    let filter = app.notes.filter.to_lowercase();
    let text = if filter.is_empty() {
        "  No notes found. Use `scribe note add` to create one."
    } else {
        "  No notes match the current filter."
    };

    let paragraph = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(paragraph, area);
}
