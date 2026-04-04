//! Generic inline form component for the Scribe TUI.
//!
//! [`Form`] is a floating popup containing an ordered list of [`FormField`]
//! items. It supports text input, single-select dropdowns, and a datetime
//! field with inline validation.
//!
//! # Key bindings (while a form is active)
//!
//! | Key | Action |
//! |-----|--------|
//! | `Tab` | Advance to next field |
//! | `Shift-Tab` | Go back to previous field |
//! | `Enter` (on last field) | Submit the form |
//! | `Esc` | Cancel and close |
//! | `j` / `k` (on Select) | Navigate options |
//! | `Left` / `Right` (on Text) | Move cursor |
//! | `Home` / `End` (on Text) | Jump to start/end |
//! | `Backspace` (on Text) | Delete character before cursor |
//! | `Char` (on Text/DateTime) | Insert character at cursor |
//!
//! Forms do not execute mutations — the caller inspects
//! [`Form::is_submitted`] and reads field values via [`Form::field_value`]
//! or [`Form::select_index`].

#[path = "form/render.rs"]
mod render;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use render::{build_field_layout, centred_form_rect, handle_text_key, render_field};

// ── FormField ─────────────────────────────────────────────────────────────

/// A single interactive field inside a [`Form`].
#[derive(Debug, Clone)]
pub enum FormField {
    /// Single-line text input.
    Text {
        /// Display label.
        label: String,
        /// Current value of the field.
        value: String,
        /// Hint shown when the field is empty.
        placeholder: String,
        /// Cursor position (byte offset into `value`).
        cursor: usize,
    },
    /// Dropdown list of string options.
    Select {
        /// Display label.
        label: String,
        /// Available options.
        options: Vec<String>,
        /// Index of the currently highlighted option.
        selected: usize,
    },
    /// ISO-8601 datetime text input with inline validation.
    DateTime {
        /// Display label.
        label: String,
        /// Current raw text value.
        value: String,
        /// Validation error message shown below the field.
        error: Option<String>,
        /// Cursor position (byte offset).
        cursor: usize,
    },
}

impl FormField {
    /// Returns the display label.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Text { label, .. }
            | Self::Select { label, .. }
            | Self::DateTime { label, .. } => label,
        }
    }

    /// Returns the current string value (for text/datetime) or the selected
    /// option string (for select).
    #[must_use]
    pub fn value_str(&self) -> &str {
        match self {
            Self::Text { value, .. } | Self::DateTime { value, .. } => value,
            Self::Select {
                options, selected, ..
            } => options.get(*selected).map_or("", String::as_str),
        }
    }

    /// Returns the selected index for a [`FormField::Select`] field.
    ///
    /// Returns `0` for non-select fields.
    #[must_use]
    pub fn select_index(&self) -> usize {
        match self {
            Self::Select { selected, .. } => *selected,
            _ => 0,
        }
    }
}

// ── Form ──────────────────────────────────────────────────────────────────

/// The submission state of a [`Form`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormState {
    /// The form is still open and being edited.
    Open,
    /// The user pressed Enter on the last field — caller should read values.
    Submitted,
    /// The user pressed Esc — caller should discard and close.
    Cancelled,
}

/// A floating form popup with labelled fields.
///
/// Construct via [`Form::new`], then call [`Form::handle_key`] on every key
/// event. Read [`Form::state`] to know when to close the form.
///
/// # Examples
///
/// ```
/// use scribe::tui::components::form::{Form, FormField};
///
/// let form = Form::new(
///     "New Todo",
///     vec![
///         FormField::Text {
///             label: "Title".into(),
///             value: String::new(),
///             placeholder: "Enter title…".into(),
///             cursor: 0,
///         },
///     ],
/// );
/// assert_eq!(form.title(), "New Todo");
/// ```
#[derive(Debug, Clone)]
pub struct Form {
    /// Form title shown in the popup border.
    title: String,
    /// Ordered list of fields.
    fields: Vec<FormField>,
    /// Index of the currently focused field.
    focused: usize,
    /// Current submission state.
    state: FormState,
}

impl Form {
    /// Creates a new [`Form`] with the given title and fields.
    ///
    /// The first field receives focus initially.
    ///
    /// # Examples
    ///
    /// ```
    /// use scribe::tui::components::form::{Form, FormField};
    ///
    /// let f = Form::new("Edit", vec![FormField::Text {
    ///     label: "Title".into(),
    ///     value: "existing".into(),
    ///     placeholder: String::new(),
    ///     cursor: 8,
    /// }]);
    /// assert_eq!(f.state(), scribe::tui::components::form::FormState::Open);
    /// ```
    #[must_use]
    pub fn new(title: impl Into<String>, fields: Vec<FormField>) -> Self {
        Self {
            title: title.into(),
            fields,
            focused: 0,
            state: FormState::Open,
        }
    }

    /// Returns the form's title string.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the current submission state.
    #[must_use]
    pub fn state(&self) -> FormState {
        self.state
    }

    /// Returns `true` if the form has been submitted.
    #[must_use]
    pub fn is_submitted(&self) -> bool {
        self.state == FormState::Submitted
    }

    /// Returns the string value of field at `index`.
    ///
    /// Returns an empty string if the index is out of range.
    #[must_use]
    pub fn field_value(&self, index: usize) -> &str {
        self.fields.get(index).map_or("", FormField::value_str)
    }

    /// Returns the selected index of field at `index` (for Select fields).
    ///
    /// Returns `0` if the index is out of range or the field is not a Select.
    #[must_use]
    pub fn select_index(&self, index: usize) -> usize {
        self.fields.get(index).map_or(0, FormField::select_index)
    }

    /// Returns a shared slice of all fields.
    #[must_use]
    pub fn fields(&self) -> &[FormField] {
        &self.fields
    }

    /// Handles a key event while the form is open.
    ///
    /// Updates internal state; the caller should re-render and check
    /// [`Form::state`] on each event.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if self.state != FormState::Open {
            return;
        }

        match code {
            KeyCode::Esc => {
                self.state = FormState::Cancelled;
            }
            KeyCode::Tab => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if self.focused > 0 {
                        self.focused -= 1;
                    }
                } else if self.focused + 1 < self.fields.len() {
                    self.focused += 1;
                }
            }
            KeyCode::Enter => {
                if self.focused + 1 >= self.fields.len() {
                    if self.validate_current() {
                        self.state = FormState::Submitted;
                    }
                } else {
                    self.focused += 1;
                }
            }
            _ => {
                self.handle_field_key(code);
            }
        }
    }

    /// Renders the form popup centred over `area`.
    ///
    /// Clears the background before drawing.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let popup = centred_form_rect(area, self.fields.len());
        frame.render_widget(Clear, popup);

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let content = inner.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });

        let rows = build_field_layout(content, self.fields.len());

        for (i, field) in self.fields.iter().enumerate() {
            render_field(frame, field, i == self.focused, &rows, i);
        }

        // Hint line at the bottom.
        let hint_idx = rows.len() - 1;
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  [Tab] Next  [Shift-Tab] Prev  [Enter] Submit  [Esc] Cancel",
                Style::default().fg(Color::DarkGray),
            )),
            rows[hint_idx],
        );
    }

    // ── private helpers ────────────────────────────────────────────────────

    /// Dispatches a key to the currently focused field.
    fn handle_field_key(&mut self, code: KeyCode) {
        let Some(field) = self.fields.get_mut(self.focused) else {
            return;
        };

        match field {
            FormField::Text { value, cursor, .. } | FormField::DateTime { value, cursor, .. } => {
                handle_text_key(value, cursor, code);
            }
            FormField::Select {
                options, selected, ..
            } => match code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if *selected + 1 < options.len() {
                        *selected += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                }
                _ => {}
            },
        }
    }

    /// Validates the currently focused field.
    ///
    /// For [`FormField::DateTime`], parses the value and stores an error hint
    /// if invalid. Returns `true` if validation passes.
    fn validate_current(&mut self) -> bool {
        let Some(field) = self.fields.get_mut(self.focused) else {
            return true;
        };

        if let FormField::DateTime { value, error, .. } = field {
            let normalized = value.replace(' ', "T");
            let normalized = if normalized.len() == 16 {
                format!("{normalized}:00")
            } else {
                normalized
            };

            if chrono::DateTime::parse_from_rfc3339(&format!("{normalized}+00:00")).is_ok()
                || chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S").is_ok()
            {
                *error = None;
                return true;
            }

            *error = Some("Expected YYYY-MM-DD HH:MM".to_owned());
            return false;
        }

        true
    }
}
