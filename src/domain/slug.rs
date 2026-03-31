// Rust guideline compliant 2026-02-21
//! Slug generation and uniqueness enforcement.
//!
//! A slug is a lowercase, kebab-case, URL-safe string used as the
//! user-facing identifier for every entity in Scribe.
//!
//! # Algorithm
//!
//! 1. Lowercase the input.
//! 2. Replace any run of non-alphanumeric characters with a single `-`.
//! 3. Strip leading and trailing `-`.
//! 4. Truncate to 40 characters at a character boundary.
//! 5. Prepend the caller-supplied prefix (e.g. `payments-task-`).
//!
//! Collision resolution appends a 4-character random alphanumeric suffix
//! and retries up to [`MAX_RETRIES`] times before returning [`SlugError`].
//!
//! # Example
//!
//! ```
//! use scribe::domain::slug;
//!
//! let s = slug::generate("payments-task-", "Fix the login bug!");
//! assert_eq!(s, "payments-task-fix-the-login-bug");
//! ```

use std::fmt;

/// Maximum number of uniqueness-retry attempts before giving up.
///
/// Chosen to make pathological collision storms extremely unlikely
/// while keeping the retry loop bounded.
// DOCUMENTED-MAGIC: 5 gives 36^4 ≈ 1.7 M distinct suffixes spread over
// at most 5 attempts; the probability of 5 consecutive collisions is
// negligible for any realistic dataset.
const MAX_RETRIES: u8 = 5;

/// Maximum number of characters in the title portion of a generated slug.
///
/// Keeps slugs readable in terminal output and avoids overlong `SQLite` UNIQUE
/// index entries.
// DOCUMENTED-MAGIC: 40 fits comfortably on an 80-column terminal alongside
// a prefix such as "payments-task-" without wrapping.
const MAX_TITLE_LEN: usize = 40;

// ── error type ─────────────────────────────────────────────────────────────

/// Errors that can arise during slug generation.
///
/// Currently the only failure mode is exhausting all uniqueness retries
/// (see [`SlugError::is_collision_limit`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlugError {
    kind: SlugErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlugErrorKind {
    /// All retry attempts produced collisions.
    CollisionLimit {
        /// The candidate slug that kept colliding.
        candidate: String,
        /// Number of attempts made.
        attempts: u8,
    },
}

impl SlugError {
    /// Returns `true` when the error is a collision-limit failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use scribe::domain::slug::{ensure_unique, SlugError};
    ///
    /// // Force immediate collision every time
    /// let result = ensure_unique("my-slug", |_| true);
    /// assert!(result.unwrap_err().is_collision_limit());
    /// ```
    // Used in tests and will be used in Phase 2+ error handling.
    #[allow(dead_code, reason = "used in tests and Phase 2+ error handling")]
    #[must_use]
    pub fn is_collision_limit(&self) -> bool {
        matches!(self.kind, SlugErrorKind::CollisionLimit { .. })
    }
}

impl fmt::Display for SlugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SlugErrorKind::CollisionLimit {
                candidate,
                attempts,
            } => write!(
                f,
                "slug '{candidate}' collided on all {attempts} attempts; \
                 the namespace may be exhausted"
            ),
        }
    }
}

impl std::error::Error for SlugError {}

// ── public API ─────────────────────────────────────────────────────────────

/// Generates a slug from `prefix` and `title`.
///
/// The title is lowercased, non-alphanumeric runs are collapsed to `-`, and
/// the result is truncated to [`MAX_TITLE_LEN`] characters before being
/// appended to `prefix`.
///
/// This function does **not** check for uniqueness — call [`ensure_unique`]
/// afterwards when inserting into the database.
///
/// # Examples
///
/// ```
/// use scribe::domain::slug;
///
/// assert_eq!(slug::generate("proj-task-", "Fix Login Bug"), "proj-task-fix-login-bug");
/// assert_eq!(slug::generate("", "hello world"), "hello-world");
/// ```
pub fn generate(prefix: &str, title: &str) -> String {
    let normalised = normalise(title);
    format!("{prefix}{normalised}")
}

/// Ensures `candidate` is unique by appending a random suffix on collision.
///
/// Calls `exists` to test whether a candidate slug is already taken.
/// On collision a 4-character random alphanumeric suffix is appended and the
/// check is retried. After [`MAX_RETRIES`] failed attempts a [`SlugError`]
/// is returned.
///
/// # Errors
///
/// Returns [`SlugError`] when all retry attempts produce collisions.
///
/// # Examples
///
/// ```
/// use scribe::domain::slug;
///
/// // No collision — returns the candidate unchanged.
/// let s = slug::ensure_unique("my-slug", |_| false).unwrap();
/// assert_eq!(s, "my-slug");
///
/// // All attempts collide.
/// let e = slug::ensure_unique("taken", |_| true).unwrap_err();
/// assert!(e.is_collision_limit());
/// ```
pub fn ensure_unique(candidate: &str, exists: impl Fn(&str) -> bool) -> Result<String, SlugError> {
    if !exists(candidate) {
        return Ok(candidate.to_owned());
    }

    for attempt in 1..=MAX_RETRIES {
        let suffix = random_suffix();
        let with_suffix = format!("{candidate}-{suffix}");
        if !exists(&with_suffix) {
            return Ok(with_suffix);
        }
        tracing::debug!(
            slug.candidate = candidate,
            slug.attempt = attempt,
            "slug collision, retrying with suffix",
        );
    }

    Err(SlugError {
        kind: SlugErrorKind::CollisionLimit {
            candidate: candidate.to_owned(),
            attempts: MAX_RETRIES,
        },
    })
}

// ── private helpers ────────────────────────────────────────────────────────

/// Lowercases, collapses non-alphanumeric runs to `-`, strips boundary `-`.
fn normalise(input: &str) -> String {
    let lower = input.to_lowercase();
    let mut slug = String::with_capacity(lower.len());
    let mut last_was_dash = false;

    for ch in lower.chars() {
        if ch.is_alphanumeric() {
            slug.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    // Strip trailing dash
    let slug = slug.trim_end_matches('-');
    // Strip leading dash
    let slug = slug.trim_start_matches('-');

    // Truncate at character boundary
    if slug.len() <= MAX_TITLE_LEN {
        slug.to_owned()
    } else {
        // Find the last '-' within the first MAX_TITLE_LEN bytes/chars.
        let boundary = slug[..MAX_TITLE_LEN].rfind('-').unwrap_or(MAX_TITLE_LEN);
        slug[..boundary].trim_end_matches('-').to_owned()
    }
}

/// Generates a 4-character random alphanumeric suffix.
fn random_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Simple LCG seeded from system time — sufficient for slug collision
    // avoidance; not a security primitive.
    // DOCUMENTED-MAGIC: LCG constants (a=6364136223846793005, c=1442695040888963407)
    // are Knuth's MMIX parameters, widely used for non-cryptographic randomness.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0xDEAD_BEEF);

    let mut state = u64::from(seed);
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let mut out = String::with_capacity(4);
    for _ in 0..4 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let idx = ((state >> 33) as usize) % chars.len();
        out.push(chars[idx]);
    }
    out
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_basic() {
        assert_eq!(
            generate("proj-task-", "Fix Login Bug"),
            "proj-task-fix-login-bug"
        );
    }

    #[test]
    fn test_generate_strips_special_characters() {
        assert_eq!(generate("", "Hello, World! (2026)"), "hello-world-2026");
    }

    #[test]
    fn test_generate_collapses_multiple_separators() {
        assert_eq!(generate("", "foo  --  bar"), "foo-bar");
    }

    #[test]
    fn test_generate_truncates_long_title() {
        let long_title = "a".repeat(100);
        let slug = generate("", &long_title);
        assert!(
            slug.len() <= MAX_TITLE_LEN,
            "slug '{slug}' exceeds {MAX_TITLE_LEN} chars"
        );
    }

    #[test]
    fn test_generate_truncates_at_word_boundary() {
        // 41 chars: "the-quick-brown-fox-jumps-over-lazy-dog-x"
        let title = "the quick brown fox jumps over lazy dog x";
        let slug = generate("", title);
        assert!(!slug.ends_with('-'), "slug must not end with '-': {slug}");
        assert!(slug.len() <= MAX_TITLE_LEN);
    }

    #[test]
    fn test_ensure_unique_no_collision() {
        let result = ensure_unique("my-slug", |_| false);
        assert_eq!(result.unwrap(), "my-slug");
    }

    #[test]
    fn test_ensure_unique_with_collision_resolves() {
        let first_call = std::cell::Cell::new(true);
        let result = ensure_unique("my-slug", |_| {
            if first_call.get() {
                first_call.set(false);
                true
            } else {
                false
            }
        });
        let slug = result.unwrap();
        assert!(slug.starts_with("my-slug-"), "expected suffix: {slug}");
    }

    #[test]
    fn test_ensure_unique_exhausted_returns_error() {
        let result = ensure_unique("taken", |_| true);
        let err = result.unwrap_err();
        assert!(err.is_collision_limit());
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_generate_empty_prefix() {
        assert_eq!(generate("", "hello world"), "hello-world");
    }

    #[test]
    fn test_generate_unicode_title() {
        // Non-ASCII chars are not alphanumeric in is_alphanumeric for ASCII? Actually
        // Rust's char::is_alphanumeric() handles unicode. Tilde and dash are not alphanumeric.
        let slug = generate("", "Café & Résumé");
        // accented letters ARE alphanumeric in Rust
        assert!(!slug.contains(' '));
        assert!(!slug.contains('&'));
    }
}
