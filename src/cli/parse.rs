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
//! | `tomorrow 09:00` / `tomorrow 9am` | Next calendar day at the given time |
//! | `today 5pm` | Today at 17:00 local |
//! | `friday 17:00` | Coming Friday at the given time |
//! | `9am` / `5:30pm` | Today at that clock time |
//! | `in 1 hour` / `in 30 minutes` | Relative to now |
//!
//! All local times are converted to UTC using `chrono`'s local timezone.

use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday,
};

// ── public API ─────────────────────────────────────────────────────────────

/// Parses a flexible datetime string into a UTC `DateTime`.
///
/// Supports ISO 8601, space-separated, relative (`tomorrow`, `today`, `in N hours`),
/// weekday shortcuts, and 12-hour clock times (`9am`). See the module documentation.
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

    let lower = s.to_lowercase();

    // ── relative: "in 1 hour" / "in 30 minutes" ───────────────────────────
    if let Some(rest) = lower.strip_prefix("in ") {
        return parse_relative_duration(rest.trim());
    }

    // ── relative: "tomorrow HH:MM" / "tomorrow 9am" ───────────────────────
    if let Some(time_part) = lower.strip_prefix("tomorrow") {
        let time_part = time_part.trim();
        let time = if time_part.is_empty() {
            NaiveTime::from_hms_opt(9, 0, 0).expect("valid 09:00")
        } else {
            parse_time(time_part)?
        };
        let tomorrow = Local::now().date_naive() + Duration::days(1);
        let ndt = tomorrow.and_time(time);
        return local_to_utc(ndt);
    }

    // ── relative: "today HH:MM" / "today 5pm" ─────────────────────────────
    if let Some(time_part) = lower.strip_prefix("today") {
        let time_part = time_part.trim();
        let time = if time_part.is_empty() {
            // "today" alone → now (rounded to next minute is unnecessary; use now)
            return Ok(Utc::now());
        } else {
            parse_time(time_part)?
        };
        let today = Local::now().date_naive();
        let ndt = today.and_time(time);
        return local_to_utc(ndt);
    }

    // ── weekday: "friday 17:00" / "friday 5pm" ────────────────────────────
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

    // ── bare clock time: "9am", "14:30" → today at that time ─────────────
    if let Ok(time) = parse_time(&lower) {
        let today = Local::now().date_naive();
        let ndt = today.and_time(time);
        return local_to_utc(ndt);
    }

    Err(anyhow::anyhow!(
        "cannot parse datetime '{s}'; \
         expected formats: '2026-04-01T14:00', '2026-04-01 14:00', \
         'tomorrow 09:00', 'tomorrow 9am', 'today 5pm', 'friday 17:00', \
         'in 1 hour', 'in 30 minutes'"
    ))
}

// ── private helpers ────────────────────────────────────────────────────────

/// Parses a relative duration from now: `1 hour`, `2 hours`, `30 minutes`, `45 mins`.
fn parse_relative_duration(s: &str) -> anyhow::Result<DateTime<Utc>> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "invalid relative duration '{s}'; expected 'N hour(s)|minute(s)'"
        ));
    }
    let amount: i64 = parts[0]
        .parse()
        .map_err(|_parse_err| anyhow::anyhow!("invalid number in relative duration '{s}'"))?;
    if amount < 0 {
        return Err(anyhow::anyhow!("relative duration must be non-negative"));
    }
    let unit = parts[1];
    let delta = match unit {
        "hour" | "hours" | "hr" | "hrs" | "h" => Duration::hours(amount),
        "minute" | "minutes" | "min" | "mins" | "m" => Duration::minutes(amount),
        "day" | "days" | "d" => Duration::days(amount),
        other => {
            return Err(anyhow::anyhow!(
                "unknown duration unit '{other}'; use hours, minutes, or days"
            ));
        }
    };
    Ok(Utc::now() + delta)
}

/// Parses `HH:MM`, `HH:MM:SS`, or 12-hour forms (`9am`, `9:30pm`, `12am`) into a `NaiveTime`.
fn parse_time(s: &str) -> anyhow::Result<NaiveTime> {
    let s = s.trim().to_lowercase().replace(' ', "");
    if s.is_empty() {
        return Err(anyhow::anyhow!("empty time"));
    }

    if let Ok(t) = NaiveTime::parse_from_str(&s, "%H:%M:%S") {
        return Ok(t);
    }
    if let Ok(t) = NaiveTime::parse_from_str(&s, "%H:%M") {
        return Ok(t);
    }

    // 12-hour: 9am, 9:30pm, 12am, 12:00pm
    let (core, is_pm) = if let Some(core) = s.strip_suffix("am") {
        (core, false)
    } else if let Some(core) = s.strip_suffix("pm") {
        (core, true)
    } else {
        return Err(anyhow::anyhow!("invalid time '{s}'"));
    };

    let (hour, minute) = if let Some((h, m)) = core.split_once(':') {
        (
            h.parse::<u32>()
                .map_err(|_parse_err| anyhow::anyhow!("invalid hour in '{s}'"))?,
            m.parse::<u32>()
                .map_err(|_parse_err| anyhow::anyhow!("invalid minute in '{s}'"))?,
        )
    } else {
        (
            core.parse::<u32>()
                .map_err(|_parse_err| anyhow::anyhow!("invalid hour in '{s}'"))?,
            0u32,
        )
    };

    if !(1..=12).contains(&hour) || minute > 59 {
        return Err(anyhow::anyhow!("invalid 12-hour time '{s}'"));
    }

    // DOCUMENTED-MAGIC: 12am → 00:00, 12pm → 12:00; 1–11pm → +12.
    let hour_24 = match (hour, is_pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    };

    NaiveTime::from_hms_opt(hour_24, minute, 0)
        .ok_or_else(|| anyhow::anyhow!("invalid time components in '{s}'"))
}

/// Converts a local naive datetime to UTC.
fn local_to_utc(ndt: NaiveDateTime) -> anyhow::Result<DateTime<Utc>> {
    let local = ndt
        .and_local_timezone(Local)
        .single()
        .ok_or_else(|| anyhow::anyhow!("ambiguous or invalid local datetime '{ndt}'"))?;
    Ok(local.with_timezone(&Utc))
}

/// Returns the date of the next occurrence of `weekday` (always ≥ 1 day ahead
/// if today is that weekday).
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
