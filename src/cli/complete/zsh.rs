// Rust guideline compliant 2026-02-21
//! Hand-authored zsh completion script for `scribe`.
//!
//! Exported as [`ZSH_COMPLETION`], a `&str` that is printed verbatim by
//! `scribe completions zsh`. The script calls `scribe __complete <entity>`
//! at every slug-valued argument position so completions are sourced live
//! from the user's database.
//!
//! The script text lives in the adjacent `scribe.zsh` file so that this
//! source file stays within the 400-line guideline limit.

/// Complete, hand-authored zsh completion script for `scribe`.
///
/// Printed verbatim by `scribe completions zsh`. Sourcing this file (or
/// placing it in a `$fpath` directory as `_scribe`) enables full
/// tab-completion including live slug candidates fetched from the database
/// via `scribe __complete <entity>`.
// DOCUMENTED-MAGIC: `_scribe_dynamic_complete` is a private helper name;
// it must be globally unique in the user's zsh environment. The `_scribe`
// prefix follows the zsh convention for private completion helpers.
pub const ZSH_COMPLETION: &str = include_str!("scribe.zsh");
