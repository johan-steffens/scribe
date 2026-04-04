//! CLI integration tests using `assert_cmd` and isolated temporary databases.
//!
//! Each test creates a `tempfile::TempDir` and sets `SCRIBE_TEST_DB` so that
//! tests do not affect each other or the user's real database.
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

// ── todo tests ─────────────────────────────────────────────────────────────

#[test]
fn test_todo_add_and_list() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["todo", "add", "Buy groceries"])
        .success()
        .stdout(predicates::str::contains("Created todo"));

    run(&dir, &["todo", "list"])
        .success()
        .stdout(predicates::str::contains("buy-groceries"));
}

#[test]
fn test_todo_add_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["todo", "add", "Read the docs", "--output", "json"])
        .success()
        .stdout(predicates::str::contains("\"slug\""))
        .stdout(predicates::str::contains("read-the-docs"));
}

#[test]
fn test_todo_add_defaults_to_quick_capture() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["todo", "add", "Default project item"])
        .success()
        .stdout(predicates::str::contains("quick-capture-todo-"));
}

#[test]
fn test_todo_done() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["todo", "add", "Finish task"]).success();

    let list_out = scribe(&dir)
        .args(["todo", "list"])
        .output()
        .expect("list todos");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("finish-task"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    run(&dir, &["todo", "done", &slug])
        .success()
        .stdout(predicates::str::contains("Done"));
}

#[test]
fn test_todo_archive_and_restore() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["todo", "add", "Archive this todo"]).success();

    let list_out = scribe(&dir)
        .args(["todo", "list"])
        .output()
        .expect("list todos");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("archive-this-todo"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    run(&dir, &["todo", "archive", &slug])
        .success()
        .stdout(predicates::str::contains("Archived"));

    run(&dir, &["todo", "restore", &slug])
        .success()
        .stdout(predicates::str::contains("Restored"));
}

#[test]
fn test_todo_delete_requires_archived() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["todo", "add", "Delete blocker"]).success();

    let list_out = scribe(&dir)
        .args(["todo", "list"])
        .output()
        .expect("list todos");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("delete-blocker"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    // Delete without archiving first must fail.
    run(&dir, &["todo", "delete", &slug])
        .failure()
        .stderr(predicates::str::contains("archived"));
}

#[test]
fn test_todo_delete_archived_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["todo", "add", "Delete archived todo"]).success();

    let list_out = scribe(&dir)
        .args(["todo", "list"])
        .output()
        .expect("list todos");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("delete-archived-todo"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    run(&dir, &["todo", "archive", &slug]).success();
    run(&dir, &["todo", "delete", &slug])
        .success()
        .stdout(predicates::str::contains("Deleted"));
}

// ── capture / inbox tests ───────────────────────────────────────────────────

#[test]
fn test_capture_creates_inbox_item() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "Remember to call the dentist"])
        .success()
        .stdout(predicates::str::contains("Captured"));

    run(&dir, &["inbox", "list"])
        .success()
        .stdout(predicates::str::contains("Remember to call the dentist"));
}

#[test]
fn test_capture_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "A thought", "--output", "json"])
        .success()
        .stdout(predicates::str::contains("\"slug\""))
        .stdout(predicates::str::contains("A thought"));
}

#[test]
fn test_capture_empty_body_fails() {
    let dir = TempDir::new().expect("tempdir");
    // An all-whitespace body must be rejected.
    run(&dir, &["capture", "   "]).failure();
}

#[test]
fn test_inbox_list_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "List me in JSON"]).success();
    run(&dir, &["inbox", "list", "--output", "json"])
        .success()
        .stdout(predicates::str::contains("\"slug\""));
}

#[test]
fn test_inbox_process_json_skips_interactive() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "Process me"]).success();

    let list_out = scribe(&dir)
        .args(["inbox", "list"])
        .output()
        .expect("list inbox");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("capture-"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    // With --output json the command returns without entering interactive mode.
    run(&dir, &["inbox", "process", &slug, "--output", "json"])
        .success()
        .stdout(predicates::str::contains("\"slug\""));
}

// ── track tests ─────────────────────────────────────────────────────────────

#[test]
fn test_track_start_and_stop() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "start"])
        .success()
        .stdout(predicates::str::contains("Started timer"));

    run(&dir, &["track", "stop"])
        .success()
        .stdout(predicates::str::contains("Stopped timer"));
}

#[test]
fn test_track_start_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "start", "--output", "json"])
        .success()
        .stdout(predicates::str::contains("\"slug\""));

    // Clean up.
    run(&dir, &["track", "stop"]).success();
}

#[test]
fn test_track_status_no_timer() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "status"])
        .success()
        .stdout(predicates::str::contains("No timer running"));
}

#[test]
fn test_track_status_running() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "start"]).success();
    run(&dir, &["track", "status"])
        .success()
        .stdout(predicates::str::contains("Running"));
    run(&dir, &["track", "stop"]).success();
}

#[test]
fn test_track_start_blocked_when_already_running() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "start"]).success();
    // Second start must fail with a message about the running timer.
    run(&dir, &["track", "start"])
        .failure()
        .stderr(predicates::str::contains("already running"));
    run(&dir, &["track", "stop"]).success();
}

#[test]
fn test_track_stop_when_no_timer_fails() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "stop"])
        .failure()
        .stderr(predicates::str::contains("no timer"));
}

#[test]
fn test_track_report() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "start"]).success();
    run(&dir, &["track", "stop"]).success();
    run(&dir, &["track", "report", "--today"])
        .success()
        .stdout(predicates::str::contains("Total"));
}

#[test]
fn test_track_report_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["track", "start"]).success();
    run(&dir, &["track", "stop"]).success();
    run(&dir, &["track", "report", "--today", "--output", "json"])
        .success()
        .stdout(predicates::str::contains("\"slug\""));
}

// ── reminder tests ─────────────────────────────────────────────────────────

#[test]
fn test_reminder_add_and_list() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "reminder",
            "add",
            "--project",
            "quick-capture",
            "--at",
            "2099-12-31T09:00",
            "--message",
            "Deploy on New Year",
        ],
    )
    .success()
    .stdout(predicates::str::contains("Created reminder"));

    run(&dir, &["reminder", "list"])
        .success()
        .stdout(predicates::str::contains("quick-capture-reminder-"));
}

#[test]
fn test_reminder_add_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "reminder",
            "add",
            "--project",
            "quick-capture",
            "--at",
            "2099-12-31T09:00",
            "--output",
            "json",
        ],
    )
    .success()
    .stdout(predicates::str::contains("\"slug\""));
}

#[test]
fn test_reminder_archive_and_restore() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "reminder",
            "add",
            "--project",
            "quick-capture",
            "--at",
            "2099-11-01T10:00",
            "--message",
            "Archive reminder test",
        ],
    )
    .success();

    let list_out = scribe(&dir)
        .args(["reminder", "list"])
        .output()
        .expect("list reminders");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("quick-capture-reminder-"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    run(&dir, &["reminder", "archive", &slug])
        .success()
        .stdout(predicates::str::contains("Archived"));

    run(&dir, &["reminder", "restore", &slug])
        .success()
        .stdout(predicates::str::contains("Restored"));
}

#[test]
fn test_reminder_delete_requires_archived() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "reminder",
            "add",
            "--project",
            "quick-capture",
            "--at",
            "2099-10-01T10:00",
            "--message",
            "Delete blocker reminder",
        ],
    )
    .success();

    let list_out = scribe(&dir)
        .args(["reminder", "list"])
        .output()
        .expect("list reminders");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("quick-capture-reminder-"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    // Delete without archiving first must fail.
    run(&dir, &["reminder", "delete", &slug])
        .failure()
        .stderr(predicates::str::contains("archived"));
}

#[test]
fn test_reminder_delete_archived_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    run(
        &dir,
        &[
            "reminder",
            "add",
            "--project",
            "quick-capture",
            "--at",
            "2099-09-01T08:00",
            "--message",
            "Delete archived reminder",
        ],
    )
    .success();

    let list_out = scribe(&dir)
        .args(["reminder", "list"])
        .output()
        .expect("list reminders");
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let slug = stdout
        .lines()
        .find(|l| l.contains("quick-capture-reminder-"))
        .and_then(|l| l.split_whitespace().next())
        .expect("slug not found")
        .to_owned();

    run(&dir, &["reminder", "archive", &slug]).success();
    run(&dir, &["reminder", "delete", &slug])
        .success()
        .stdout(predicates::str::contains("Deleted"));
}
