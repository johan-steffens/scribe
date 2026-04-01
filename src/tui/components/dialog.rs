// Rust guideline compliant 2026-02-21
//! Confirmation dialog component.
//!
//! [`ConfirmDialog`] is a small modal that asks the user a yes/no question
//! before executing a destructive action. It renders as a floating popup
//! centred over the main content area.
//!
//! # Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `y` / `Enter` | Confirm |
//! | `n` / `Esc` | Cancel |
//!
//! The dialog does **not** execute the action itself — the caller stores a
//! callback slug/context in a companion field and performs the mutation in the
//! key handler after `ConfirmDialog::state()` returns `Confirmed`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

// ── types ──────────────────────────────────────────────────────────────────

/// User's response to a confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResponse {
    /// The dialog is still visible; the user has not responded yet.
    Pending,
    /// The user pressed `y` or `Enter`.
    Confirmed,
    /// The user pressed `n` or `Esc`.
    Cancelled,
}

/// A two-option yes/no confirmation modal.
///
/// Construct via [`ConfirmDialog::new`], then call [`ConfirmDialog::handle_key`]
/// on every key event while the modal is visible.
///
/// # Examples
///
/// ```
/// use scribe::tui::components::dialog::ConfirmDialog;
///
/// let dialog = ConfirmDialog::new("Archive this todo?");
/// assert_eq!(dialog.message(), "Archive this todo?");
/// ```
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    /// The question displayed to the user.
    message: String,
}

impl ConfirmDialog {
    /// Creates a new [`ConfirmDialog`] with the given message.
    ///
    /// # Examples
    ///
    /// ```
    /// use scribe::tui::components::dialog::ConfirmDialog;
    ///
    /// let dialog = ConfirmDialog::new("Are you sure?");
    /// ```
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the dialog's message string.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Handles a key press and returns the resulting [`DialogResponse`].
    ///
    /// `y` / `Enter` → [`DialogResponse::Confirmed`],
    /// `n` / `Esc` → [`DialogResponse::Cancelled`],
    /// any other key → [`DialogResponse::Pending`].
    #[must_use]
    pub fn handle_key(code: crossterm::event::KeyCode) -> DialogResponse {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Char('y') | KeyCode::Enter => DialogResponse::Confirmed,
            KeyCode::Char('n') | KeyCode::Esc => DialogResponse::Cancelled,
            _ => DialogResponse::Pending,
        }
    }

    /// Renders the confirmation dialog centred over `area`.
    ///
    /// Clears the background before drawing so the popup is always readable.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        // Size the popup: fixed 40 % wide, 5 rows tall.
        // DOCUMENTED-MAGIC: 40 % width keeps the popup small and non-intrusive
        // while still fitting messages up to ~50 characters.
        let popup = centred_rect(40, area);
        frame.render_widget(Clear, popup);

        let block = Block::default()
            .title(" Confirm ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let rows = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);

        let msg_line = Paragraph::new(Line::from(Span::styled(
            format!("  {}", self.message()),
            Style::default().fg(Color::White),
        )));
        frame.render_widget(msg_line, rows[1]);

        let hint_line = Paragraph::new(Line::from(vec![
            Span::styled("  [", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Y",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("] Yes  [", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "N",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("] No", Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(hint_line, rows[2]);
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

/// Returns a centred [`Rect`] of the given percentage width and a fixed 5-row
/// height, vertically positioned in the upper-middle of `area`.
fn centred_rect(percent_x: u16, area: Rect) -> Rect {
    // Fixed 5-row height for the dialog box.
    // DOCUMENTED-MAGIC: 5 rows = 1 border + 1 blank + 1 message + 1 hints + 1 border.
    const DIALOG_HEIGHT: u16 = 5;
    let top = area.height.saturating_sub(DIALOG_HEIGHT) / 3;

    let vertical = Layout::vertical([
        Constraint::Length(top),
        Constraint::Length(DIALOG_HEIGHT),
        Constraint::Fill(1),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;

    #[test]
    fn test_confirm_on_y() {
        assert_eq!(
            ConfirmDialog::handle_key(KeyCode::Char('y')),
            DialogResponse::Confirmed
        );
    }

    #[test]
    fn test_confirm_on_enter() {
        assert_eq!(
            ConfirmDialog::handle_key(KeyCode::Enter),
            DialogResponse::Confirmed
        );
    }

    #[test]
    fn test_cancel_on_n() {
        assert_eq!(
            ConfirmDialog::handle_key(KeyCode::Char('n')),
            DialogResponse::Cancelled
        );
    }

    #[test]
    fn test_cancel_on_esc() {
        assert_eq!(
            ConfirmDialog::handle_key(KeyCode::Esc),
            DialogResponse::Cancelled
        );
    }

    #[test]
    fn test_other_key_pending() {
        assert_eq!(
            ConfirmDialog::handle_key(KeyCode::Char('x')),
            DialogResponse::Pending
        );
    }

    #[test]
    fn test_message_accessor() {
        let d = ConfirmDialog::new("Are you sure?");
        assert_eq!(d.message(), "Are you sure?");
    }
}
