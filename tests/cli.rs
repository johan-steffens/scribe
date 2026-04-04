//! CLI handler integration tests.
//!
//! Tests CLI command handlers (`scribe agent install`, `scribe capture`,
//! `scribe inbox list/process`, `scribe setup --wizard`, `scribe sync configure`)
//! by spawning the actual binary and asserting on stdout/stderr output.
//!
//! Each test uses isolated temporary directories for config and database
//! to prevent interference.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ── scribe command helper ─────────────────────────────────────────────────────

/// Returns a `Command` for the `scribe` binary with an isolated DB.
fn scribe_with_db(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("scribe").expect("binary not found");
    cmd.env("SCRIBE_TEST_DB", dir.path().join("test.db"));
    // Use a mock keychain bootstrap file to avoid real OS keychain prompts
    cmd.env(
        "SCRIBE_TEST_KEYCHAIN_BOOTSTRAP",
        dir.path().join("mock-keychain.json"),
    );
    cmd.timeout(std::time::Duration::from_secs(5));
    cmd
}

/// Returns a `Command` for the `scribe` binary with an isolated home directory.
fn scribe_with_home(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("scribe").expect("binary not found");
    cmd.env("HOME", home.path());
    // Also isolate the keychain bootstrap file in the home directory
    cmd.env(
        "SCRIBE_TEST_KEYCHAIN_BOOTSTRAP",
        home.path().join("mock-keychain.json"),
    );
    cmd.timeout(std::time::Duration::from_secs(5));
    cmd
}

/// Returns a `Command` for the `scribe` binary with isolated config and DB.
fn scribe_with_config(home: &TempDir, db_path: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("scribe").expect("binary not found");
    cmd.env("HOME", home.path());
    cmd.env("SCRIBE_TEST_DB", db_path.path().join("test.db"));
    // Also isolate the keychain bootstrap file
    cmd.env(
        "SCRIBE_TEST_KEYCHAIN_BOOTSTRAP",
        db_path.path().join("mock-keychain.json"),
    );
    cmd.timeout(std::time::Duration::from_secs(5));
    cmd
}

/// Helper to run scribe with DB and capture the result.
fn run(dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    scribe_with_db(dir).args(args).assert()
}

// ── Agent installer tests ─────────────────────────────────────────────────────

#[test]
fn test_agent_install_skips_missing_directories() {
    let home = TempDir::new().expect("tempdir");
    scribe_with_home(&home)
        .args(["agent", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped"))
        .stdout(predicate::str::contains("Claude Code"));
}

#[test]
fn test_agent_install_creates_skill_file_in_claude_directory() {
    let home = TempDir::new().expect("tempdir");
    let claude_dir = home.path().join(".claude/skills");
    std::fs::create_dir_all(&claude_dir).expect("create claude dir");

    scribe_with_home(&home)
        .args(["agent", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed skill for Claude Code"));

    let skill_file = claude_dir.join("scribe.md");
    assert!(
        skill_file.exists(),
        "skill file should exist at {skill_file:?}",
    );
    let content = std::fs::read_to_string(&skill_file).expect("read skill file");
    assert!(
        content.contains("Scribe is an offline-first"),
        "skill file should contain Scribe description"
    );
}

#[test]
fn test_agent_install_multiple_agents() {
    let home = TempDir::new().expect("tempdir");

    // Create multiple agent directories
    let claude_dir = home.path().join(".claude/skills");
    let cursor_dir = home.path().join(".cursor/rules");
    std::fs::create_dir_all(&claude_dir).expect("create claude dir");
    std::fs::create_dir_all(&cursor_dir).expect("create cursor dir");

    scribe_with_home(&home)
        .args(["agent", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed skill for Claude Code"))
        .stdout(predicate::str::contains("Installed skill for Cursor"));

    assert!(
        claude_dir.join("scribe.md").exists(),
        "claude skill file should exist"
    );
    assert!(
        cursor_dir.join("scribe.md").exists(),
        "cursor skill file should exist"
    );
}

#[test]
fn test_agent_install_json_output() {
    let home = TempDir::new().expect("tempdir");
    let claude_dir = home.path().join(".claude/skills");
    std::fs::create_dir_all(&claude_dir).expect("create claude dir");

    scribe_with_home(&home)
        .args(["agent", "install", "--output", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"installed\""))
        .stdout(predicate::str::contains("\"agent\": \"Claude Code"));
}

// ── Capture tests ─────────────────────────────────────────────────────────────

#[test]
fn test_capture_basic() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "My first capture"])
        .success()
        .stdout(predicate::str::contains("Captured: My first capture"))
        .stdout(predicate::str::contains("capture-"));
}

#[test]
fn test_capture_empty_text_fails() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", ""]).failure();
}

#[test]
fn test_capture_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "JSON capture", "--output", "json"])
        .success()
        .stdout(predicate::str::contains("\"body\""))
        .stdout(predicate::str::contains("\"slug\""));
}

#[test]
fn test_capture_whitespace_only_fails() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "   "]).failure();
}

// ── Inbox list tests ─────────────────────────────────────────────────────────

#[test]
fn test_inbox_list_empty() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["inbox", "list"])
        .success()
        .stdout(predicate::str::contains("Inbox is empty"));
}

#[test]
fn test_inbox_list_after_capture() {
    let dir = TempDir::new().expect("tempdir");
    // Create a capture first
    run(&dir, &["capture", "Inbox item for listing"]).success();

    run(&dir, &["inbox", "list"])
        .success()
        .stdout(predicate::str::contains("Inbox item for listing"));
}

#[test]
fn test_inbox_list_json_output() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "JSON list test"]).success();

    run(&dir, &["inbox", "list", "--output", "json"])
        .success()
        .stdout(predicate::str::contains("\"body\""))
        .stdout(predicate::str::contains("\"slug\""));
}

// ── Inbox process tests ───────────────────────────────────────────────────────

#[test]
fn test_inbox_process_json_non_interactive() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "Process via JSON"]).success();

    // Get the slug
    let output = scribe_with_db(&dir)
        .args(["inbox", "list", "--output", "json"])
        .output()
        .expect("list inbox");
    let stdout = String::from_utf8_lossy(&output.stdout);

    if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout)
        && let Some(item) = items.first()
    {
        let slug = item["slug"].as_str().unwrap_or("");
        // Process with JSON output (non-interactive)
        run(&dir, &["inbox", "process", slug, "--output", "json"])
            .success()
            .stdout(predicate::str::contains("\"slug\""));
    }
}

#[test]
fn test_inbox_process_interactive_discard() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["capture", "Interactive discard test"]).success();

    let output = scribe_with_db(&dir)
        .args(["inbox", "list", "--output", "json"])
        .output()
        .expect("list inbox");
    let stdout = String::from_utf8_lossy(&output.stdout);

    if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout)
        && let Some(item) = items.first()
    {
        let slug = item["slug"].as_str().unwrap_or("");
        // Process interactively: choose '4' for discard
        scribe_with_db(&dir)
            .args(["inbox", "process", slug])
            .write_stdin("4\n")
            .assert()
            .success()
            .stdout(predicate::str::contains("Processed:"));
    }
}

#[test]
fn test_mcp_cli_help() {
    let dir = TempDir::new().expect("tempdir");
    run(&dir, &["mcp", "--help"])
        .success()
        .stdout(predicate::str::contains("Run the Scribe MCP stdio server"));
}

// ── Setup wizard tests ────────────────────────────────────────────────────────

#[test]
fn test_setup_status_first_run() {
    let home = TempDir::new().expect("tempdir");
    scribe_with_home(&home)
        .args(["setup", "--status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not installed"));
}

#[test]
fn test_setup_wizard_accepts_defaults() {
    let home = TempDir::new().expect("tempdir");
    // Run wizard with defaults: answer 'n' to daemon service, 'n' to agent integration
    scribe_with_home(&home)
        .args(["setup", "--wizard"])
        .write_stdin("\n\n") // accept defaults for prompts
        .assert()
        .success();
}

#[test]
fn test_setup_wizard_installs_agent() {
    let home = TempDir::new().expect("tempdir");
    // Create the claude directory so agent install succeeds
    let claude_dir = home.path().join(".claude/skills");
    std::fs::create_dir_all(&claude_dir).expect("create claude dir");

    // Run wizard: 'n' to daemon, 'y' to agent
    scribe_with_home(&home)
        .args(["setup", "--wizard"])
        .write_stdin("n\ny\n") // no to daemon, yes to agent
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed skill for Claude Code"));
}

#[test]
fn test_setup_status_after_agent_install() {
    let home = TempDir::new().expect("tempdir");
    // Create the claude directory
    let claude_dir = home.path().join(".claude/skills");
    std::fs::create_dir_all(&claude_dir).expect("create claude dir");

    // First install agent
    scribe_with_home(&home)
        .args(["agent", "install"])
        .assert()
        .success();

    // Then check status
    scribe_with_home(&home)
        .args(["setup", "--status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed")); // agent should be listed as installed
}

// ── Sync configure tests ──────────────────────────────────────────────────────

// NOTE: The following tests are skipped because `rpassword::prompt_password`
// reads from /dev/tty which is unavailable in the test environment.
// - test_sync_configure_gist_with_token (requires password prompt)
// - test_sync_configure_jsonbin (requires password prompt)
// - test_sync_configure_rest_client (requires password prompt for secret)
// - test_sync_configure_removes_secrets (requires gist config first)
// - test_sync_status_after_configure (requires gist config first)

#[test]
fn test_sync_configure_rest_master() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    // Configure REST master
    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "rest"])
        .write_stdin("1\n\n") // choose master role, default port
        .assert()
        .success()
        .stdout(predicate::str::contains("REST master configured"));
}

// NOTE: The following tests are commented out because `rpassword::prompt_password`
// reads from /dev/tty which is unavailable in the test environment.
// - test_sync_configure_gist_with_token (requires password prompt)
// - test_sync_configure_jsonbin (requires password prompt)
// - test_sync_configure_rest_client (requires password prompt for secret)
// - test_sync_configure_removes_secrets (requires gist config first)
// - test_sync_status_after_configure (requires gist config first)

#[test]
fn test_sync_configure_invalid_provider_fails() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "invalid"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown provider"));
}

#[test]
fn test_sync_status_disabled() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["sync", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sync enabled:  false"));
}

#[test]
fn test_sync_one_shot_without_config_fails() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["sync"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sync is not enabled"));
}

// ── Sync push/pull (file provider) tests ────────────────────────────────────────

#[test]
fn test_sync_push_creates_remote_state_file() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    // Configure file sync.
    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    // Now run sync (push) - this should create the sync file.
    scribe_with_config(&home, &db)
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync complete"));
}

#[test]
fn test_sync_push_updates_existing_state_file() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    // Configure and run sync twice.
    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    // First sync.
    scribe_with_config(&home, &db)
        .args(["sync"])
        .assert()
        .success();

    // Second sync should also succeed (update).
    scribe_with_config(&home, &db)
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync complete"));
}

#[test]
fn test_sync_push_with_data_persists_entities() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    // Configure file sync.
    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    // Create some data first.
    scribe_with_config(&home, &db)
        .args(["project", "add", "test-proj", "--name", "Test Project"])
        .assert()
        .success();

    // Run sync to push the data.
    scribe_with_config(&home, &db)
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync complete"));

    // Verify the sync file was created and contains the project.
    let content = std::fs::read_to_string(&sync_file).expect("sync file should exist");
    assert!(
        content.contains("test-proj"),
        "sync file should contain project slug"
    );
}

#[test]
fn test_sync_push_json_output() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    scribe_with_config(&home, &db)
        .args(["sync", "--output", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"enabled\""))
        .stdout(predicate::str::contains("\"provider\""));
}

#[test]
fn test_sync_status_shows_provider_after_configure() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    scribe_with_config(&home, &db)
        .args(["sync", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sync enabled:  true"))
        .stdout(predicate::str::contains("provider:      file"));
}

#[test]
fn test_sync_status_json_output() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    scribe_with_config(&home, &db)
        .args(["sync", "status", "--output", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"enabled\": true"))
        .stdout(predicate::str::contains("\"provider\":"));
}

#[test]
fn test_sync_configure_file_provider() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync configured successfully"));
}

#[test]
fn test_sync_configure_remove_secrets() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    // First configure.
    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    // Then remove secrets.
    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--remove"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync secrets removed"));
}

#[test]
fn test_sync_pull_reads_from_existing_file() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    // Configure file sync.
    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    // Create a pre-existing sync file with state.
    let pre_existing_state = serde_json::json!({
        "snapshot_at": "2024-01-01T00:00:00Z",
        "machine_id": "00000000-0000-0000-0000-000000000000",
        "schema_version": 1,
        "projects": [{
            "id": 1,
            "slug": "pre-existing-project",
            "name": "Pre-existing Project",
            "description": null,
            "status": "active",
            "is_reserved": false,
            "archived_at": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }],
        "tasks": [],
        "todos": [],
        "time_entries": [],
        "reminders": [],
        "capture_items": []
    });
    std::fs::write(
        &sync_file,
        serde_json::to_string_pretty(&pre_existing_state).unwrap(),
    )
    .expect("should write pre-existing state");

    // Run sync - should pull the pre-existing project.
    scribe_with_config(&home, &db)
        .args(["sync"])
        .assert()
        .success();

    // The project should now be in the database.
    scribe_with_config(&home, &db)
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pre-existing-project"));
}

#[test]
fn test_sync_one_shot_succeeds_after_configure() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");
    let sync_file = db.path().join("scribe-state.json");

    scribe_with_config(&home, &db)
        .args(["sync", "configure", "--provider", "file"])
        .write_stdin(format!("{}\n", sync_file.display()))
        .assert()
        .success();

    // One-shot sync should work.
    scribe_with_config(&home, &db)
        .args(["sync"])
        .assert()
        .success();
}

// ── Service status tests ───────────────────────────────────────────────────────

#[test]
fn test_service_status_shows_not_installed_on_first_run() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["service", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Daemon service diagnostic"))
        .stdout(predicate::str::contains("Config flag:"));
}

#[test]
fn test_service_status_contains_expected_fields() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    let output = scribe_with_config(&home, &db)
        .args(["service", "status"])
        .assert()
        .success();

    // Should contain diagnostic sections.
    let stdout = output.get_output().stdout.clone();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(
        stdout_str.contains("Config flag:"),
        "should show config flag"
    );
    assert!(
        stdout_str.contains("Service file:"),
        "should show service file status"
    );
    assert!(
        stdout_str.contains("Process running:"),
        "should show process status"
    );
}

#[test]
fn test_service_subcommand_help() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["service", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Run the background reminder notification daemon directly",
        ))
        .stdout(predicate::str::contains(
            "Install and start the background reminder daemon service",
        ))
        .stdout(predicate::str::contains(
            "Stop and remove the background reminder daemon service",
        ))
        .stdout(predicate::str::contains(
            "Show whether the background daemon service is currently installed",
        ))
        .stdout(predicate::str::contains(
            "Restart the background daemon service",
        ));
}

// Note: test_service_run_subcommand_help is removed because `scribe service run`
// starts the daemon which would block the test. The service run behavior is
// tested indirectly through the daemon loop code.

#[test]
fn test_service_unknown_subcommand_fails() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["service", "unknown"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized"));
}

#[test]
fn test_service_with_invalid_flag_fails() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["service", "status", "--invalid-flag"])
        .assert()
        .failure();
}

#[test]
fn test_service_reinstall_help() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["service", "reinstall", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reinstall the daemon service"))
        .stdout(predicate::str::contains("uninstalls and reinstalls"));
}

#[test]
fn test_service_restart_help() {
    let home = TempDir::new().expect("tempdir");
    let db = TempDir::new().expect("tempdir");

    scribe_with_config(&home, &db)
        .args(["service", "restart", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Restart the background daemon service",
        ));
}
