// Rust guideline compliant 2026-02-21
//! OS desktop-notification helpers for Scribe reminders.
//!
//! [`fire`] sends a single native desktop notification. The implementation is
//! platform-specific:
//!
//! - **macOS** — shells out to `osascript`. `NSUserNotificationCenter`
//!   (the backend used by `notify-rust`) is deprecated since macOS 12 and
//!   non-functional on macOS 26+, so we use `osascript` which works on every
//!   Mac without any bundle or permission setup.
//! - **Linux / BSD** — uses `notify-rust` over D-Bus / XDG.
//! - **Windows** — uses `notify-rust` over `WinRT`.
//!
//! Failures are logged at `WARN` level and never propagated — a missed
//! notification is a minor annoyance, not a fatal error.

use crate::domain::Reminder;

// DOCUMENTED-MAGIC: "Scribe Reminder" is the fixed notification title shown
// in Notification Centre on all platforms. Changing it will affect users who
// have configured per-app notification rules for this string.
const NOTIFICATION_TITLE: &str = "Scribe Reminder";

/// Sends a desktop notification for a fired reminder.
///
/// The notification title is always `"Scribe Reminder"`. The body is the
/// reminder's message if set, otherwise `"Reminder due"`.
///
/// Delivery errors are logged at `WARN` level and swallowed.
///
/// # Examples
///
/// ```no_run
/// # use scribe::domain::Reminder;
/// # fn example(reminder: &Reminder) {
/// scribe::notify::fire(reminder);
/// # }
/// ```
pub fn fire(reminder: &Reminder) {
    let body = reminder.message.as_deref().unwrap_or("Reminder due");
    fire_impl(reminder, body);
}

// ── macOS — osascript ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn fire_impl(reminder: &Reminder, body: &str) {
    use std::process::Command;

    // AppleScript string literals use double quotes only; escape any in content.
    let safe_body = body.replace('"', "\\\"");
    let safe_title = NOTIFICATION_TITLE.replace('"', "\\\"");

    // DOCUMENTED-MAGIC: persistent reminders use `display alert` which blocks
    // until the user clicks Dismiss (or the `giving up after 0` expression
    // resolves — 0 means "never time out"). Non-persistent reminders use
    // `display notification` which auto-dismisses after ~5 s.
    // Both paths are attributed to "Script Editor" in Notification Centre;
    // the user controls alert style in System Settings → Notifications →
    // Script Editor.
    let script = if reminder.persistent {
        format!(
            "display alert \"{safe_title}\" message \"{safe_body}\" \
             buttons {{\"Dismiss\"}} giving up after 0",
        )
    } else {
        format!("display notification \"{safe_body}\" with title \"{safe_title}\"",)
    };

    match Command::new("osascript").args(["-e", &script]).status() {
        Ok(status) if status.success() => {
            tracing::info!(
                reminder.slug = %reminder.slug,
                "reminder.notification.sent",
            );
        }
        Ok(status) => {
            tracing::warn!(
                reminder.slug = %reminder.slug,
                exit_code = ?status.code(),
                "reminder.notification.failed",
            );
        }
        Err(e) => {
            tracing::warn!(
                reminder.slug = %reminder.slug,
                error = %e,
                "reminder.notification.osascript_not_found",
            );
        }
    }
}

// ── Linux / Windows — notify-rust ─────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
fn fire_impl(reminder: &Reminder, body: &str) {
    let result = notify_rust::Notification::new()
        .appname("Scribe")
        .summary(NOTIFICATION_TITLE)
        .body(body)
        // DOCUMENTED-MAGIC: Timeout::Default uses the platform default display
        // duration (~5 s banner on most Linux DEs; persistent on some).
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
