//! E013 and E014: the identifier contract of entity IDs. Every test starts from `.spec` text
//! (ADR 0115).

use specforge_common::DiagnosticData;
use specforge_test_macros::test as spec;

use crate::support::written::checked;
use crate::support::{build, coded_in, software};

/// `behavior <id> "S" { }` for each id, in one file.
fn behaviors(ids: &[&str]) -> String {
    ids.iter()
        .map(|id| format!("behavior {id} \"S\" {{\n}}\n\n"))
        .collect()
}

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "an entity ID equal to a structural keyword or a registered kind's keyword is E013"
)]
fn a_reserved_entity_id_produces_e013() {
    let diags = checked(
        &build([software()]),
        &[("t.spec", &behaviors(&["behavior"]))],
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
    for keyword in ["spec", "ref", "use", "define", "defaults"] {
        let diags = checked(&build, &[("t.spec", &behaviors(&[keyword]))]);
        assert_eq!(coded_in(&diags, "E013").len(), 1, "{keyword}: {diags:?}");
    }
}

#[test]
fn normal_identifiers_pass_the_reserved_check() {
    let text = format!("{}spec \"my_project\" {{\n}}\n", behaviors(&["login_flow"]));
    let diags = checked(&build([software()]), &[("t.spec", &text)]);
    assert!(coded_in(&diags, "E013").is_empty(), "{diags:?}");
}

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "an entity ID shorter than 2 or longer than 60 characters is E014"
)]
fn identifier_length_bounds() {
    let build = build([software()]);
    let short = checked(&build, &[("t.spec", &behaviors(&["x"]))]);
    assert_eq!(coded_in(&short, "E014").len(), 1, "{short:?}");

    let long_id = "a".repeat(61);
    let long = checked(&build, &[("t.spec", &behaviors(&[&long_id]))]);
    assert_eq!(coded_in(&long, "E014").len(), 1, "{long:?}");
}

#[test]
fn identifier_length_bounds_are_inclusive() {
    let sixty = "a".repeat(60);
    let diags = checked(
        &build([software()]),
        &[("t.spec", &behaviors(&["ab", &sixty]))],
    );
    assert!(coded_in(&diags, "E014").is_empty(), "{diags:?}");
}
