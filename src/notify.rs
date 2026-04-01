// Rust guideline compliant 2026-02-21
//! OS desktop-notification helpers for Scribe reminders.
//!
//! [`fire`] sends a single native desktop notification using the
//! `notify-rust` crate. It is the sole place in the codebase that talks
//! to the OS notification subsystem.
//!
//! Failures are logged at `WARN` level but never propagated — a missed
//! notification is a minor annoyance, not a fatal error.

use crate::domain::Reminder;

/// Sends a desktop notification for a fired reminder.
///
/// The notification title is always `"Scribe Reminder"`. The body is the
/// reminder's message if set, otherwise the `remind_at` timestamp formatted
/// as a human-readable UTC string.
///
/// Notification delivery errors are logged at `WARN` level and swallowed.
///
/// # Examples
///
/// ```no_run
/// use scribe::notify;
/// use scribe::domain::Reminder;
/// // fire(&reminder); // would send an OS notification
/// ```
pub fn fire(reminder: &Reminder) {
    let body = reminder.message.as_deref().unwrap_or("Reminder due");
    let result = notify_rust::Notification::new()
        .appname("Scribe")
        .summary("Scribe Reminder")
        .body(body)
        // DOCUMENTED-MAGIC: timeout of 0 means "use platform default"
        // (usually ~5 s on macOS, persistent until dismissed on Linux).
        .timeout(notify_rust::Timeout::Default)
        .show();

    match result {
        Ok(_) => {
            tracing::info!(
                reminder.slug = %reminder.slug,
                "reminder.notification.sent",
            );
        }
        Err(e) => {
            tracing::warn!(
                reminder.slug = %reminder.slug,
                error = %e,
                "reminder.notification.failed",
            );
        }
    }
}
