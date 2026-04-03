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
# Must pass with zero warnings
cargo clippy --all-features -- -D warnings

# Must be formatted
cargo fmt --all

# All tests must pass
cargo test --all-features
```

Run all checks locally before pushing:

```sh
cargo fmt --all && cargo clippy --all-features -- -D warnings && cargo test --all-features
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
```

## Writing Tests

Unit tests live alongside their source files. Integration tests live in
`tests/`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_function() {
        assert_eq!(my_function(1), 2);
    }
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
