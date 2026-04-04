//! Unit tests for [`crate::ops::projects::ProjectOps`].

use scribe::domain::{NewProject, ProjectStatus};
use scribe::testing::project_ops;

use project_ops::ops as make_ops;

fn new_project(slug: &str) -> NewProject {
    NewProject {
        slug: slug.to_owned(),
        name: slug.to_owned(),
        description: None,
        status: ProjectStatus::Active,
    }
}

#[test]
fn test_create_and_get_project() {
    let ops = make_ops();
    let p = ops.create_project(new_project("beta")).expect("create");
    assert_eq!(p.slug, "beta");
    let got = ops.get_project("beta").expect("get").expect("some");
    assert_eq!(got.id, p.id);
}

#[test]
fn test_delete_reserved_blocked() {
    let ops = make_ops();
    let err = ops.delete_project("quick-capture").unwrap_err();
    assert!(err.to_string().contains("reserved"));
}
