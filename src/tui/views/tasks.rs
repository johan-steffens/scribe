//! Tasks view — tree-structured list of active tasks.
//!
//! Each row shows:
//! `[priority badge]  [status badge]  [tree indent] title  [project-slug]  [due date]`
//!
//! Top-level items are parent tasks. Pressing `Enter` or `Right Arrow` on a
//! parent task expands it to reveal its sub-tasks (checklist items). The `Left`
//! Arrow collapses an expanded task.
//!
//! Live filter via `/` narrows by title or project-slug substring match.
//! The selected row is highlighted.
//!
//! This is a pure rendering function; no state is mutated here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::domain::Task;
use crate::domain::task::{TaskPriority, TaskStatus};
use crate::tui::app::App;

/// A visible task row in the tree, with its depth and tree-drawing info.
#[derive(Debug)]
struct TaskRow {
    task: Task,
    /// 0 for top-level, 1+ for nested children.
    depth: usize,
    /// True if this task is the last child at its depth.
    is_last_child: bool,
}

/// Renders the tasks tree into `area`.
///
/// Applies the current filter string from `app.tasks.filter`. The highlighted
/// row index is `app.tasks.selected`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let visible = build_visible_tree(app);

    if visible.is_empty() {
        let filter = app.tasks.filter.to_lowercase();
        let text = if filter.is_empty() {
            "  No tasks found. Use `scribe task add` to create one."
        } else {
            "  No tasks match the current filter."
        };
        let paragraph = ratatui::widgets::Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(paragraph, area);
        return;
    }

    let rows: Vec<Vec<String>> = visible
        .iter()
        .map(|row| {
            let project_slug = row.task.project_slug.clone();

            let due = row
                .task
                .due_date
                .map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string());

            let tree_prefix = build_tree_prefix(row.depth, row.is_last_child);

            vec![
                priority_badge(row.task.priority).to_owned(),
                status_badge(row.task.status).to_owned(),
                tree_prefix,
                row.task.title.clone(),
                project_slug,
                due,
            ]
        })
        .collect();

    let selected = app.tasks.selected.min(visible.len().saturating_sub(1));

    let header_cells: Vec<Cell<'_>> = ["Pri", "Status", "", "Title", "Project", "Due"]
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
        .height(1);

    let data_rows: Vec<Row<'_>> = rows
        .into_iter()
        .map(|cells| {
            let row_cells: Vec<Cell<'_>> = cells.into_iter().map(Cell::from).collect();
            Row::new(row_cells).height(1)
        })
        .collect();

    let table = Table::new(data_rows, task_constraints())
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

/// Column constraints for the tasks table.
fn task_constraints() -> [Constraint; 6] {
    [
        Constraint::Length(4),
        Constraint::Length(11),
        Constraint::Length(4), // tree indent column
        Constraint::Min(24),
        Constraint::Min(16),
        Constraint::Length(10),
    ]
}

/// Builds the list of visible task rows in tree order.
fn build_visible_tree(app: &App) -> Vec<TaskRow> {
    let ctx = TreeContext::new(app);

    // Collect top-level tasks
    let top_level: Vec<&Task> = app
        .tasks
        .items
        .iter()
        .filter(|t| t.archived_at.is_none() && t.parent_id.is_none())
        .filter(|t| {
            if ctx.filter.is_empty() {
                true
            } else {
                t.title.to_lowercase().contains(&ctx.filter)
                    || t.project_slug.to_lowercase().contains(&ctx.filter)
            }
        })
        .collect();

    // Build visible tree
    let mut result = Vec::new();
    for (idx, task) in top_level.iter().enumerate() {
        let is_last = idx == top_level.len() - 1;
        add_task_row(&mut result, task, 0, is_last, &ctx);
    }

    result
}

/// Context for tree traversal.
struct TreeContext<'a> {
    children_map: std::collections::HashMap<crate::domain::TaskId, Vec<&'a Task>>,
    filter: String,
    app: &'a App,
}

impl<'a> TreeContext<'a> {
    fn new(app: &'a App) -> Self {
        let filter = app.tasks.filter.to_lowercase();

        // Collect all visible tasks with their parent info
        let all_tasks: Vec<&Task> = app
            .tasks
            .items
            .iter()
            .filter(|t| t.archived_at.is_none())
            .collect();

        // Build a map of parent_id -> children
        let mut children_map: std::collections::HashMap<crate::domain::TaskId, Vec<&Task>> =
            std::collections::HashMap::new();
        for task in &all_tasks {
            if let Some(parent_id) = task.parent_id {
                children_map.entry(parent_id).or_default().push(task);
            }
        }

        Self {
            children_map,
            filter,
            app,
        }
    }
}

/// Recursively adds a task and its visible children to the result list.
fn add_task_row(
    result: &mut Vec<TaskRow>,
    task: &Task,
    depth: usize,
    is_last_child: bool,
    ctx: &TreeContext,
) {
    result.push(TaskRow {
        task: task.clone(),
        depth,
        is_last_child,
    });

    // If this task is not expanded, don't show children
    if !ctx.app.tasks.expanded_tasks.contains(&task.id) {
        return;
    }

    // Get all children
    let Some(all_children) = ctx.children_map.get(&task.id) else {
        return;
    };

    let total_children = all_children.len();

    // Iterate through all children, adding only those that pass the filter
    for (idx, child) in all_children.iter().enumerate() {
        let passes_filter = ctx.filter.is_empty()
            || child.title.to_lowercase().contains(&ctx.filter)
            || child.project_slug.to_lowercase().contains(&ctx.filter);

        if passes_filter {
            // is_last_child means this is the last of ALL siblings (unfiltered)
            let child_is_last = idx == total_children - 1;
            add_task_row(result, child, depth + 1, child_is_last, ctx);
        }
    }
}

/// Builds the tree prefix string for indentation and branch drawing.
fn build_tree_prefix(depth: usize, is_last_child: bool) -> String {
    if depth == 0 {
        return String::new();
    }

    let mut prefix = String::new();
    for _ in 0..depth {
        // Use space for intermediate depths
        prefix.push_str("  ");
    }

    // Add tree branch character
    if is_last_child {
        prefix.push_str("└─");
    } else {
        prefix.push_str("├─");
    }

    prefix
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

/// Returns a short status badge string for display.
const fn status_badge(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Todo => "todo      ",
        TaskStatus::InProgress => "in_progres",
        TaskStatus::Done => "done      ",
        TaskStatus::Cancelled => "cancelled ",
    }
}
