//! One finder answers which tokens name an entity (ADR 0163): rename, the navigator's tokens and a delete's
//! referrer scan read a default no block takes, and a type name a derived field reads.

use crate::navigate::{SOFTWARE, compile};
use specforge_ops::authoring::rename;

const RULES: &str = "invariant inv_a \"A\" {\n  guarantee \"g\"\n}\n";
/// A file whose only block is a default no block takes (W161).
const FLOWS: &str = "defaults behavior {\n  invariants [inv_a]\n}\n";

#[test]
#[ignore = "flipped by T5"]
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
