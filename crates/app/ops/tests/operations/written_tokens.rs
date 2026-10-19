//! One finder answers which tokens name an entity (ADR 0163): rename, the navigator's tokens and a delete's
//! referrer scan read a default no block takes, and a type name a derived field reads.

use crate::navigate::{SOFTWARE, at, compile};
use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::{Direction, ReferenceQuery};
use specforge_test_macros::test as specforge_test;

const INCOMING: ReferenceQuery = ReferenceQuery {
    direction: Direction::Incoming,
    include_declaration: false,
};

const RULES: &str = "invariant inv_a \"A\" {\n  guarantee \"g\"\n}\n";
/// A file whose only block is a default no block takes (W161).
const FLOWS: &str = "defaults behavior {\n  invariants [inv_a]\n}\n";

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "rename edits a default no block takes"
)]
fn rename_edits_a_default_no_block_takes() {
    let p = compile(SOFTWARE, &[("rules.spec", RULES), ("flows.spec", FLOWS)]);
    let plan = rename::plan(&p.navigator(), "inv_a", "inv_b").unwrap();
    let files = plan.affected_files();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        ["flows.spec", "rules.spec"]
    );
    let flows = plan.edits.iter().find(|e| e.file == "flows.spec").unwrap();
    assert_eq!((flows.line, flows.start_col, flows.end_col), (2, 14, 19));
}

#[test]
fn tokens_hold_a_default_no_block_takes() {
    let p = compile(SOFTWARE, &[("rules.spec", RULES), ("flows.spec", FLOWS)]);
    let nav = p.navigator();
    let mut found: Vec<String> = nav
        .tokens("inv_a", true)
        .unwrap()
        .iter()
        .map(|t| at(&t.span))
        .collect();
    found.sort();
    assert_eq!(found, ["flows.spec 2:15-2:20", "rules.spec 1:11-1:16"]);
    // No block holds it: the holders stay on the graph's edges (ADR 0163).
    assert!(nav.references("inv_a", INCOMING).unwrap().is_empty());
}
