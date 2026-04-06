# Contributing to Scribe

Thank you for your interest in contributing to Scribe!

## Development Setup

```sh
git clone https://github.com/johan-steffens/scribe.git
cd scribe
cargo install --path .
```

To include optional features:

```sh
cargo install --path . --features mcp,sync
```

## Workflow

Scribe uses a merge-commit-only policy on `main`. **Never rebase** — history is
preserved to trace the true sequence of events.

```sh
# Create a feature or bugfix branch
git checkout -b bugfix/your-bug-name
git checkout -b feat/your-feature-name

# Make your changes, commit using conventional commits
git commit -m "fix(sync): resolve remote slugs to local IDs"

# Push and create a PR
git push -u origin your-branch-name
gh pr create
```

## Conventional Commits

Scribe follows the [Conventional Commits](https://www.conventionalcommits.org/)
specification:

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Formatting, no code change |
| `refactor` | Code restructuring, no behavior change |
| `perf` | Performance improvement |
| `chore` | Build, CI, dependencies |
| `test` | Adding or fixing tests |

Format: `type(scope): description`

Examples:
- `feat(sync): add REST master server`
- `fix(tui): handle empty project list gracefully`
- `docs(readme): add installation instructions`

## Quality Requirements

All commits must pass before merging:

```sh
# Must be formatted
cargo fmt --all -- --check

# Must pass with zero warnings across all targets
cargo clippy --all-targets --all-features -- -D warnings

# All tests must pass
cargo test --all-features

# Coverage must be 50% or higher (target 80%+)
cargo llvm-cov --all-features --workspace --fail-under-lines 50
```

Run all checks locally before pushing:

```sh
cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features && cargo llvm-cov --all-features --workspace --fail-under-lines 50
```

## Project Structure

```
src/
├── cli/          # Clap subcommand definitions
├── db/           # SQLite connection and migrations
├── domain/       # Core domain types (Slug, etc.)
├── notify/       # Desktop notification handling
├── ops/          # High-level operations (business logic)
├── server/       # REST sync server handlers
├── store/        # Data access layer (SQLite queries)
├── sync/         # Sync engine and providers
└── tui/          # Terminal UI

tests/
├── tests/*.rs    # Unit tests (migrated from src/)
├── tui/          # TUI integration tests (TestBackend)
└── sync/         # Sync integration tests
```

**Note:** `src/` is primarily test-free (no `#[test]` blocks). All tests live
in `tests/`. Internal exposition for testing is provided via a
`#[cfg(test)] pub mod testing` pattern in each module and
re-exported in `src/testing/mod.rs`.

## Writing Tests

Tests are organized by subsystem in `tests/`:

```sh
# Unit tests
tests/todo_store_tests.rs

# TUI integration tests (ratatui TestBackend)
tests/tui/dashboard_tests.rs

# Sync integration tests
tests/sync/provider_tests.rs
```

Use helpers from `scribe::testing` for temp databases and mock configs:

```rust
use scribe::testing::db::TestDb;
use scribe::testing::config::TestConfig;

#[test]
fn test_my_function() {
    let test_db = TestDb::new();
    let conn = test_db.conn();
    // test code
}
```

Internal types are exposed for testing via the `testing` module in each module:

```rust
#[cfg(test)]
pub mod testing {
    pub use crate::ops::todos::TodoOps; // etc.
}
```

## Submitting Changes

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes with clear, atomic commits
4. Ensure `cargo clippy --all-features -- -D warnings` passes
5. Open a pull request against `main`
6. Link any related issues in the PR description

## Getting Help

If you have questions or want to discuss a change before implementing, feel free
to open an issue or start a discussion on GitHub.
