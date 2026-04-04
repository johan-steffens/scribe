//! Unit tests for [`crate::ops::inbox::InboxOps`].

use scribe::ops::inbox::ProcessAction;
use scribe::testing::inbox_ops;

use inbox_ops::ops as make_ops;

#[test]
fn test_capture_creates_item() {
    let ops = make_ops();
    let item = ops.capture("Buy groceries").expect("capture");
    assert_eq!(item.body, "Buy groceries");
    assert!(!item.processed);
    assert!(item.slug.starts_with("capture-"));
}

#[test]
fn test_capture_trims_whitespace() {
    let ops = make_ops();
    let item = ops.capture("  hello  ").expect("capture");
    assert_eq!(item.body, "hello");
}

#[test]
fn test_capture_empty_body_returns_error() {
    let ops = make_ops();
    let err = ops.capture("   ").unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn test_process_discard() {
    let ops = make_ops();
    let item = ops.capture("Discard me").expect("capture");
    let processed = ops
        .process(&item.slug, ProcessAction::Discard)
        .expect("process");
    assert!(processed.processed);
}

#[test]
fn test_process_convert_to_todo() {
    let ops = make_ops();
    let item = ops.capture("Convert to todo").expect("capture");
    let processed = ops
        .process(
            &item.slug,
            ProcessAction::ConvertToTodo {
                project_slug: "quick-capture".to_owned(),
                title: None,
            },
        )
        .expect("process");
    assert!(processed.processed);
}

#[test]
fn test_process_project_not_found_returns_error() {
    let ops = make_ops();
    let item = ops.capture("No project").expect("capture");
    let err = ops.process(&item.slug, ProcessAction::Discard).map(|_| ());
    // Discard always succeeds
    assert!(
        err.is_ok(),
        "Discard should have succeeded, but got error: {:?}",
        err.err()
    );

    let item2 = ops.capture("No project 2").expect("capture");
    let err2 = ops
        .process(
            &item2.slug,
            ProcessAction::AssignToProject {
                project_slug: "nonexistent".to_owned(),
            },
        )
        .unwrap_err();
    assert!(err2.to_string().contains("not found"));
}
