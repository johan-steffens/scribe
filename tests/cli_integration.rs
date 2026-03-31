// Rust guideline compliant 2026-02-21
//! CLI integration tests using `assert_cmd` and isolated temporary databases.
//!
//! Each test creates a `tempfile::TempDir` and sets `SCRIBE_DB` (via the
//! `--db` environment override) so that tests do not affect each other or the
//! user's real database.
//!
//! # Setup
//!
//! The binary under test must be built (`cargo test` compiles it automatically).

use assert_cmd::Command;
use tempfile::TempDir;

// ── helpers ────────────────────────────────────────────────────────────────

/// Returns a `Command` for the `scribe` binary pointed at an isolated DB.
fn scribe(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("scribe").expect("binary not found");
    cmd.env("SCRIBE_TEST_DB", dir.path().join("test.db"));
    cmd
}

/// A helper that runs `scribe` with the given args using a per-call temp dir.
fn run(dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    scribe(dir).args(args).assert()
}

// ── project tests ──────────────────────────────────────────────────────────

#[test]
fn test_project_add_and_list() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["project", "add", "my-proj", "--name", "My Project"])
        .success()
        .stdout(predicates::str::contains("Created project"));

    run(&dir, &["project", "list"])
        .success()
        .stdout(predicates::str::contains("my-proj"));
}

#[test]
fn test_project_add_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "project",
            "add",
            "json-proj",
            "--name",
            "JSON Project",
            "--output",
            "json",
        ],
    )
    .success()
    .stdout(predicates::str::contains("\"slug\""))
    .stdout(predicates::str::contains("json-proj"));
}

#[test]
fn test_project_show() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &["project", "add", "show-proj", "--name", "Show Project"],
    )
    .success();
    run(&dir, &["project", "show", "show-proj"])
        .success()
        .stdout(predicates::str::contains("show-proj"));
}

#[test]
fn test_project_archive_and_restore() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &["project", "add", "arch-proj", "--name", "Archive Me"],
    )
    .success();
    run(&dir, &["project", "archive", "arch-proj"])
        .success()
        .stdout(predicates::str::contains("Archived"));
    run(&dir, &["project", "restore", "arch-proj"])
        .success()
        .stdout(predicates::str::contains("Restored"));
}

#[test]
fn test_project_delete_reserved_fails() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["project", "delete", "quick-capture"]).failure();
}

// ── task tests ─────────────────────────────────────────────────────────────

#[test]
fn test_task_add_and_list() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "task",
            "add",
            "Write unit tests",
            "--project",
            "quick-capture",
        ],
    )
    .success()
    .stdout(predicates::str::contains("Created task"));

    run(&dir, &["task", "list"])
        .success()
        .stdout(predicates::str::contains("write-unit-tests"));
}

#[test]
fn test_task_add_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "task",
            "add",
            "Fix the bug",
            "--project",
            "quick-capture",
            "--output",
            "json",
        ],
    )
    .success()
    .stdout(predicates::str::contains("\"slug\""))
    .stdout(predicates::str::contains("fix-the-bug"));
}

#[test]
fn test_task_done() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "task",
            "add",
            "Finish something",
            "--project",
            "quick-capture",
        ],
    )
    .success();

    // Get the slug from list output
    let list_out = scribe(&dir)
        .args(["task", "list"])
        .output()
        .expect("list tasks");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("finish-something"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    run(&dir, &["task", "done", &slug])
        .success()
        .stdout(predicates::str::contains("Done"));
}

#[test]
fn test_task_add_to_custom_project() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["project", "add", "custom", "--name", "Custom"]).success();
    run(&dir, &["task", "add", "My task", "--project", "custom"])
        .success()
        .stdout(predicates::str::contains("custom-task-my-task"));
}
