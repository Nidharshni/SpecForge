//! A rename's new ID is refused before anything is planned when `check` would refuse it.

use specforge_ops::OpErrorKind;
use specforge_ops::authoring::rename::{self, RenameField};

const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

#[specforge_test_macros::test(
    behavior = "rename_entity_id",
    verify = "rename refuses a reserved word as the new ID before anything is planned"
)]
fn a_rename_to_a_reserved_word_is_refused_about_the_new_id() {
    let p = crate::navigate::compile(
        crate::navigate::SOFTWARE,
        &[("limit.spec", LIMIT), ("login.spec", LOGIN)],
    );
    // Two kinds the software extension declares, and two structural keywords.
    for reserved in ["behavior", "invariant", "defaults", "spec"] {
        let refused = rename::plan(&p.navigator(), "session_limit", reserved).unwrap_err();
        assert_eq!(refused.error.code, rename::INVALID_ID, "{reserved}");
        assert_eq!(refused.error.kind, OpErrorKind::InvalidInput, "{reserved}");
        assert_eq!(refused.subject, Some(RenameField::NewId), "{reserved}");
        assert!(
            refused.error.message.contains("reserved"),
            "{}",
            refused.error.message
        );
    }
}

#[test]
#[ignore = "flipped by the fix of the unparsed-source refusal"]
fn a_rename_is_refused_while_a_source_that_spells_the_id_does_not_parse() {
    let p = crate::navigate::compile(
        crate::navigate::SOFTWARE,
        &[
            ("limit.spec", LIMIT),
            (
                "login.spec",
                "behavior login \"Login\" {\n  invariants [session_limit\n",
            ),
        ],
    );
    let refused = rename::plan(&p.navigator(), "session_limit", "session_cap").unwrap_err();
    assert_eq!(refused.error.code, "source_unparsed");
    assert!(
        refused.error.message.contains("login.spec"),
        "{}",
        refused.error.message
    );
    // The ID inside a string of a source that does not parse is not a reference.
    let p = crate::navigate::compile(
        crate::navigate::SOFTWARE,
        &[
            ("limit.spec", LIMIT),
            (
                "other.spec",
                "behavior other \"O\" {\n  contract \"session_limit\"\n",
            ),
        ],
    );
    let plan = rename::plan(&p.navigator(), "session_limit", "session_cap").unwrap();
    assert_eq!(plan.edits.len(), 1);
}
