//! A rename's new ID is refused before anything is planned when `check` would refuse it.

use specforge_ops::OpErrorKind;
use specforge_ops::authoring::rename::{self, RenameField};

const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

#[test]
#[ignore = "flipped by T2"]
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
