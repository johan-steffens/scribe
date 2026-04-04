//! Unit tests for [`crate::domain::slug`].

use scribe::testing::slug::{MAX_TITLE_LEN, ensure_unique, generate};

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
