// Rust guideline compliant 2026-02-21
//! Unit tests for [`crate::cli::parse`] — datetime parsing.

use chrono::{Duration, Local};
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
    assert_eq!(dt.format("%Y-%m-%d").to_string(), expected);
}

#[test]
fn test_parse_weekday() {
    // Any weekday should parse without error and produce a date >= tomorrow.
    let dt = parse_datetime("friday 17:00").expect("parse");
    let tomorrow = Local::now().date_naive() + Duration::days(1);
    assert!(dt.date_naive() >= tomorrow);
}

#[test]
fn test_parse_invalid_returns_error() {
    let err = parse_datetime("not a date").unwrap_err();
    assert!(!err.to_string().is_empty());
}
