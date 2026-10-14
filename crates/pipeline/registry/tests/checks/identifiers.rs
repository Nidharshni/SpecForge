//! E013 and E014: the identifier contract of entity IDs.

use specforge_common::DiagnosticData;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use crate::support::{build, check, coded_in, software, span};

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "an entity ID equal to a structural keyword or a registered kind's keyword is E013"
)]
fn a_reserved_entity_id_produces_e013() {
    let diags = check(
        &build([software()]),
        &[EntityRecord::new("behavior", "behavior", span("t.spec"))],
    );
    let e013 = coded_in(&diags, "E013");
    assert_eq!(e013.len(), 1, "{diags:?}");
    assert!(e013[0].message.contains("behavior"));
    assert_eq!(
        e013[0].data,
        Some(Box::new(DiagnosticData::ShadowedKeyword {
            keyword: "behavior".into()
        }))
    );
    assert!(e013[0].suggestion.as_ref().unwrap().contains("rename"));
}

/// The structural keywords are reserved wherever the kinds are checked
/// (with no kind registered nothing is: the gate).
#[test]
fn a_structural_keyword_is_reserved_wherever_kinds_are_checked() {
    let build = build([software()]);
    for keyword in ["spec", "ref", "use", "define"] {
        let diags = check(
            &build,
            &[EntityRecord::new("behavior", keyword, span("t.spec"))],
        );
        assert_eq!(coded_in(&diags, "E013").len(), 1, "{keyword}: {diags:?}");
    }
}

#[test]
fn normal_identifiers_pass_the_reserved_check() {
    let diags = check(
        &build([software()]),
        &[
            EntityRecord::new("behavior", "login_flow", span("t.spec")),
            EntityRecord::new("spec", "my_project", span("t.spec")),
        ],
    );
    assert!(coded_in(&diags, "E013").is_empty(), "{diags:?}");
}

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "an entity ID shorter than 2 or longer than 60 characters is E014"
)]
fn identifier_length_bounds() {
    let build = build([software()]);
    let short = check(
        &build,
        &[EntityRecord::new("behavior", "x", span("t.spec"))],
    );
    assert_eq!(coded_in(&short, "E014").len(), 1, "{short:?}");

    let long_id = "a".repeat(61);
    let long = check(
        &build,
        &[EntityRecord::new("behavior", &long_id, span("t.spec"))],
    );
    assert_eq!(coded_in(&long, "E014").len(), 1, "{long:?}");
}

#[test]
fn identifier_length_bounds_are_inclusive() {
    let sixty = "a".repeat(60);
    let diags = check(
        &build([software()]),
        &[
            EntityRecord::new("behavior", "ab", span("t.spec")),
            EntityRecord::new("behavior", &sixty, span("t.spec")),
        ],
    );
    assert!(coded_in(&diags, "E014").is_empty(), "{diags:?}");
}
