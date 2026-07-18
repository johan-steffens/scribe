//! Unit tests for [`crate::cli::parse`] — datetime parsing.

use chrono::{Duration, Local, Timelike};
use scribe::cli::parse::parse_datetime;

#[test]
fn test_parse_iso8601_with_t() {
    let dt = parse_datetime("2026-04-01T14:00:00").expect("parse");
    // The assertion checks the date portion; UTC offset may shift the hour
    // depending on local timezone.
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-04-01");
}

#[test]
fn test_parse_space_separated() {
    let dt = parse_datetime("2026-04-01 14:00").expect("parse");
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-04-01");
}

#[test]
fn test_parse_date_only() {
    let dt = parse_datetime("2026-04-01").expect("parse");
    // The parsed UTC date may be the day before in UTC if local timezone is UTC+.
    // Check that the local date matches "2026-04-01".
    let local_date = dt.with_timezone(&Local).format("%Y-%m-%d").to_string();
    assert_eq!(local_date, "2026-04-01");
}

#[test]
fn test_parse_tomorrow() {
    let dt = parse_datetime("tomorrow 09:00").expect("parse");
    let expected = (Local::now().date_naive() + Duration::days(1)).to_string();
    // Compare in local calendar date (UTC conversion may shift the day).
    let local_date = dt.with_timezone(&Local).format("%Y-%m-%d").to_string();
    assert_eq!(local_date, expected);
}

#[test]
fn test_parse_tomorrow_9am() {
    let dt = parse_datetime("tomorrow 9am").expect("parse");
    let local = dt.with_timezone(&Local);
    let expected_date = Local::now().date_naive() + Duration::days(1);
    assert_eq!(local.date_naive(), expected_date);
    assert_eq!(local.hour(), 9);
    assert_eq!(local.minute(), 0);
}

#[test]
fn test_parse_today_5pm() {
    let dt = parse_datetime("today 5pm").expect("parse");
    let local = dt.with_timezone(&Local);
    assert_eq!(local.date_naive(), Local::now().date_naive());
    assert_eq!(local.hour(), 17);
}

#[test]
fn test_parse_bare_9am() {
    let dt = parse_datetime("9am").expect("parse");
    let local = dt.with_timezone(&Local);
    assert_eq!(local.hour(), 9);
    assert_eq!(local.minute(), 0);
}

#[test]
fn test_parse_in_1_hour() {
    let before = chrono::Utc::now();
    let dt = parse_datetime("in 1 hour").expect("parse");
    let after = chrono::Utc::now();
    // Allow a few seconds of clock skew around the parse.
    let lower = before + Duration::minutes(59);
    let upper = after + Duration::minutes(61);
    assert!(
        dt >= lower && dt <= upper,
        "got {dt}, expected ~1h from now"
    );
}

#[test]
fn test_parse_in_30_minutes() {
    let before = chrono::Utc::now();
    let dt = parse_datetime("in 30 minutes").expect("parse");
    let after = chrono::Utc::now();
    let lower = before + Duration::minutes(29);
    let upper = after + Duration::minutes(31);
    assert!(
        dt >= lower && dt <= upper,
        "got {dt}, expected ~30m from now"
    );
}

#[test]
fn test_parse_weekday() {
    // Any weekday should parse without error and produce a date >= tomorrow.
    let dt = parse_datetime("friday 17:00").expect("parse");
    let tomorrow = Local::now().date_naive() + Duration::days(1);
    assert!(dt.with_timezone(&Local).date_naive() >= tomorrow);
}

#[test]
fn test_parse_invalid_returns_error() {
    let err = parse_datetime("not a date").unwrap_err();
    assert!(!err.to_string().is_empty());
}
