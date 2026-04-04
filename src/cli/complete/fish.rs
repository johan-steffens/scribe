//! Hand-authored fish completion script for `scribe`.
//!
//! Exported as [`FISH_COMPLETION`], a `&str` that is printed verbatim by
//! `scribe completions fish`. The directives call `scribe __complete <entity>`
//! for every slug-valued argument position so completions are sourced live
//! from the user's database.
//!
//! The script text lives in the adjacent `scribe.fish` file so that this
//! source file stays within the 400-line guideline limit.

/// Complete, hand-authored fish completion script for `scribe`.
///
/// Printed verbatim by `scribe completions fish`. Add this file to
/// `~/.config/fish/completions/scribe.fish` (or source it) to enable full
/// tab-completion including live slug candidates fetched via
/// `scribe __complete <entity>`.
// DOCUMENTED-MAGIC: Fish uses `\t` as the native separator between a
// completion candidate and its description string, which matches exactly
// the output format of `scribe __complete`. No transformation is needed.
pub const FISH_COMPLETION: &str = include_str!("scribe.fish");
