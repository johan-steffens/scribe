//! Flexible datetime string parsing for CLI `--at` arguments.
//!
//! [`parse_datetime`] accepts a variety of human-friendly datetime formats
//! and converts them to a UTC [`DateTime`].
//!
//! # Supported Formats
//!
//! | Input example | Interpretation |
//! |---|---|
//! | `2026-04-01T14:00:00` | ISO 8601, treated as local time |
//! | `2026-04-01 14:00` | Space separator, no seconds, local time |
//! | `2026-04-01` | Date only, midnight local time |
//! | `tomorrow 09:00` | Next calendar day at the given time |
//! | `friday 17:00` | Coming Friday at the given time |
//!
//! All local times are converted to UTC using `chrono`'s local timezone.

use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday,
};

// ── public API ─────────────────────────────────────────────────────────────

/// Parses a flexible datetime string into a UTC `DateTime`.
///
/// Supports ISO 8601, space-separated, `tomorrow HH:MM`, and weekday
/// shortcuts. See the module documentation for the full list.
///
/// # Errors
///
/// Returns an error if the string format is not recognized or represents an
/// invalid date.
///
/// # Panics
///
/// Panics if "00:00:00" or "09:00:00" cannot be parsed as valid times.
pub fn parse_datetime(s: &str) -> anyhow::Result<DateTime<Utc>> {
    let s = s.trim();

    // ── try ISO 8601 with T separator ──────────────────────────────────────
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return local_to_utc(ndt);
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return local_to_utc(ndt);
    }

    // ── try space-separated datetime ───────────────────────────────────────
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return local_to_utc(ndt);
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return local_to_utc(ndt);
    }

    // ── try date only (midnight) ───────────────────────────────────────────
    if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let ndt = nd.and_time(NaiveTime::from_hms_opt(0, 0, 0).expect("valid midnight"));
        return local_to_utc(ndt);
    }

    // ── try relative: "tomorrow HH:MM" ────────────────────────────────────
    let lower = s.to_lowercase();
    if let Some(time_part) = lower.strip_prefix("tomorrow") {
        let time_part = time_part.trim();
        let time = parse_time(time_part)?;
        let tomorrow = Local::now().date_naive() + Duration::days(1);
        let ndt = tomorrow.and_time(time);
        return local_to_utc(ndt);
    }
    if lower == "tomorrow" {
        let tomorrow = Local::now().date_naive() + Duration::days(1);
        let ndt = tomorrow.and_time(NaiveTime::from_hms_opt(9, 0, 0).expect("valid 09:00"));
        return local_to_utc(ndt);
    }

    // ── try weekday: "friday 17:00" ────────────────────────────────────────
    let weekdays = [
        ("monday", Weekday::Mon),
        ("tuesday", Weekday::Tue),
        ("wednesday", Weekday::Wed),
        ("thursday", Weekday::Thu),
        ("friday", Weekday::Fri),
        ("saturday", Weekday::Sat),
        ("sunday", Weekday::Sun),
    ];

    for (name, weekday) in weekdays {
        if let Some(rest) = lower.strip_prefix(name) {
            let rest = rest.trim();
            let time = if rest.is_empty() {
                NaiveTime::from_hms_opt(9, 0, 0).expect("valid 09:00")
            } else {
                parse_time(rest)?
            };
            let date = next_weekday(weekday);
            let ndt = date.and_time(time);
            return local_to_utc(ndt);
        }
    }

    Err(anyhow::anyhow!(
        "cannot parse datetime '{s}'; \
         expected formats: '2026-04-01T14:00', '2026-04-01 14:00', \
         'tomorrow 09:00', 'friday 17:00'"
    ))
}

// ── private helpers ────────────────────────────────────────────────────────

/// Parses `HH:MM` or `HH:MM:SS` into a `NaiveTime`.
fn parse_time(s: &str) -> anyhow::Result<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M"))
        .map_err(|e| anyhow::anyhow!("invalid time '{s}': {e}"))
}

/// Converts a local naive datetime to UTC.
fn local_to_utc(ndt: NaiveDateTime) -> anyhow::Result<DateTime<Utc>> {
    let local = ndt
        .and_local_timezone(Local)
        .single()
        .ok_or_else(|| anyhow::anyhow!("ambiguous or invalid local datetime '{ndt}'"))?;
    Ok(local.with_timezone(&Utc))
}

/// Returns the date of the next occurrence of `weekday` (today counts only
/// if today *is* that weekday and the given time is still in the future;
/// for simplicity we always take the next occurrence ≥ 1 day ahead).
fn next_weekday(weekday: Weekday) -> NaiveDate {
    let today = Local::now().date_naive();
    let today_wd = today.weekday();
    let days_ahead = {
        let diff =
            i64::from(weekday.num_days_from_monday()) - i64::from(today_wd.num_days_from_monday());
        if diff <= 0 { diff + 7 } else { diff }
    };
    today + Duration::days(days_ahead)
}
