// Rust guideline compliant 2026-02-21
//! TUI entry point and event loop.
//!
//! The [`run`] function is the sole public API of this module. It:
//! 1. Sets up the crossterm raw-mode alternate screen terminal.
//! 2. Constructs [`App`] and populates initial data.
//! 3. Spawns a background thread that polls for due reminders and fires OS
//!    desktop notifications (via [`crate::notify`]).
//! 4. Runs the event loop, polling crossterm for key events every 250 ms.
//! 5. Restores the terminal unconditionally on exit (even on error).
//!
//! # Error handling
//!
//! Terminal setup/teardown errors are propagated as `anyhow::Error`. Runtime
//! errors (DB failures, etc.) are stored in [`App::last_error`] and displayed
//! in the status bar — the TUI never panics.
//!
//! # Examples
//!
//! ```no_run
//! # use std::sync::{Arc, Mutex};
//! # use scribe::db::open_in_memory;
//! # use scribe::config::Config;
//! use scribe::tui;
//!
//! let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
//! let cfg  = Config::default();
//! // tui::run(conn, &cfg).unwrap(); // would open the TUI
//! ```

pub mod app;
pub mod components;
#[path = "keys/mod.rs"]
pub(crate) mod keys;
pub(crate) mod types;
pub mod ui;
pub mod views;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use rusqlite::Connection;

use crate::config::Config;
use crate::ops::ReminderOps;
use crate::tui::app::App;

// DOCUMENTED-MAGIC: 250 ms poll timeout gives a smooth 4 Hz refresh rate for
// the timer display while keeping CPU usage negligible.
const POLL_TIMEOUT_MS: u64 = 250;

// DOCUMENTED-MAGIC: 30 s reminder-check interval in the TUI background thread
// mirrors the daemon default so reminders fire within one interval of their
// due time. Shorter values increase DB reads with diminishing accuracy gains.
const REMINDER_CHECK_INTERVAL_SECS: u64 = 30;

/// Launches the Scribe TUI, blocking until the user quits.
///
/// Sets up a raw-mode alternate-screen terminal, spawns a background reminder
/// thread, runs the event loop, and restores the terminal on exit — even when
/// an error occurs.
///
/// `db` is the shared `SQLite` connection. `config` is currently unused in
/// Phase 3 but is threaded through for Phase 4+.
///
/// # Errors
///
/// Returns an error if terminal setup or teardown fails. Runtime errors (DB
/// access, etc.) are displayed in the status bar and do not propagate here.
pub fn run(db: Arc<Mutex<Connection>>, _config: &Config) -> anyhow::Result<()> {
    // ── terminal setup ─────────────────────────────────────────────────────
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    // ── spawn background reminder-notification thread ──────────────────────
    // The thread owns a clone of the DB Arc and runs independently; it is
    // intentionally leaked (detached) because the TUI process exits when the
    // user quits, which terminates all threads automatically.
    spawn_reminder_thread(Arc::clone(&db));

    // ── run the event loop ─────────────────────────────────────────────────
    let result = event_loop(&mut terminal, db);

    // ── unconditional terminal teardown ────────────────────────────────────
    // Must happen even if the event loop returned an error.
    let teardown = teardown_terminal(&mut terminal);

    // Return the first error encountered (event loop errors take priority).
    result.and(teardown)
}

// ── private helpers ────────────────────────────────────────────────────────

/// Spawns a background thread that polls for due reminders every
/// [`REMINDER_CHECK_INTERVAL_SECS`] seconds and fires OS notifications.
///
/// The thread is detached — it will be terminated when the process exits.
fn spawn_reminder_thread(db: Arc<Mutex<Connection>>) {
    let _handle = thread::Builder::new()
        .name("scribe-reminder-poll".to_owned())
        .spawn(move || {
            let ops = ReminderOps::new(db);
            loop {
                thread::sleep(Duration::from_secs(REMINDER_CHECK_INTERVAL_SECS));
                match ops.check_due() {
                    Ok(fired) => {
                        for reminder in &fired {
                            tracing::info!(
                                reminder.slug = %reminder.slug,
                                "tui.reminder.fired",
                            );
                            crate::notify::fire(reminder);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "tui.reminder.poll.error");
                    }
                }
            }
        });
    // Ignore the JoinHandle — the thread is intentionally detached.
}

/// Runs the main event loop until `app.should_quit` is true.
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    db: Arc<Mutex<Connection>>,
) -> anyhow::Result<()> {
    let mut app = App::new(db);

    loop {
        // Refresh the active timer on every iteration (low-cost DB read).
        app.tick();

        // Draw the current frame.
        terminal
            .draw(|frame| ui::draw(frame, &app))
            .context("terminal draw")?;

        // Poll for a crossterm event with a timeout.
        if event::poll(std::time::Duration::from_millis(POLL_TIMEOUT_MS)).context("event poll")? {
            match event::read().context("event read")? {
                Event::Key(key) => {
                    // Ignore key release events on platforms that emit them.
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key);
                    }
                }
                // Resize and other events: just redraw on next iteration.
                Event::Resize(_, _)
                | Event::Mouse(_)
                | Event::FocusGained
                | Event::FocusLost
                | Event::Paste(_) => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Restores the terminal to its pre-TUI state.
fn teardown_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<()> {
    disable_raw_mode().context("disable raw mode")?;
    terminal
        .backend_mut()
        .execute(LeaveAlternateScreen)
        .context("leave alternate screen")?;
    terminal.show_cursor().context("show cursor")?;
    Ok(())
}
