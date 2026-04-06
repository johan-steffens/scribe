//! CLI report command integration tests.
//!
//! Tests the `scribe report` command by spawning the actual binary
//! and asserting on stdout/stderr output.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ── scribe command helper ─────────────────────────────────────────────────────

/// Returns a `Command` for the `scribe` binary with an isolated DB.
fn scribe_with_db(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("scribe").expect("binary not found");
    cmd.env("SCRIBE_TEST_DB", dir.path().join("test.db"));
    cmd.env("SCRIBE_MOCK_NOTIFY", "1");
    cmd.timeout(std::time::Duration::from_secs(10));
    cmd
}

/// Helper to run scribe with DB and capture the result.
fn run(dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    scribe_with_db(dir).args(args).assert()
}

// ── Report command tests ──────────────────────────────────────────────────────

#[test]
fn test_report_inbox_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "inbox"])
        .success()
        .stdout(predicate::str::contains("Inbox Status Report"));
}

#[test]
fn test_report_reminders_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "reminders"])
        .success()
        .stdout(predicate::str::contains("Reminders Status Report"));
}

#[test]
fn test_report_inbox_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "inbox", "--output", "json"])
        .success()
        .stdout(predicate::str::contains("\"unprocessed_items\""));
}

#[test]
fn test_report_reminders_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "reminders", "--output", "json"])
        .success()
        .stdout(predicate::str::contains("\"active_reminders\""));
}

#[test]
fn test_report_none_defaults_to_inbox() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report"])
        .success()
        .stdout(predicate::str::contains("Inbox Status Report"));
}

#[test]
fn test_report_none_with_today_flag_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "--today"])
        .success()
        .stdout(predicate::str::contains("Inbox Status Report"));
}

#[test]
fn test_report_none_with_week_flag_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "--week"])
        .success()
        .stdout(predicate::str::contains("Inbox Status Report"));
}

#[test]
fn test_report_none_with_detailed_flag_is_accepted() {
    // The --detailed flag is accepted but inbox reports don't have a detailed mode,
    // so it simply shows the standard inbox report
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "--detailed"])
        .success()
        .stdout(predicate::str::contains("Inbox Status Report"));
}

#[test]
fn test_report_project_not_found_returns_error() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "project", "nonexistent-project"])
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_report_task_not_found_returns_error() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "task", "nonexistent-task"])
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_report_todo_shows_not_implemented_message() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "todo", "some-todo"])
        .success()
        .stdout(predicate::str::contains("not yet implemented"));
}

#[test]
fn test_report_help_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "--help"])
        .success()
        .stdout(predicate::str::contains("Generate reports"));
}

#[test]
fn test_report_project_subcommand_help() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["report", "project", "--help"])
        .success()
        .stdout(predicate::str::contains("Project slug"));
}
