//! A copy of a `GraphBuild` shares its parses and owns its graph (ADR 0108).

use specforge_graph::{FileChange, GraphBuild, GraphConfig};
use specforge_parser::parse;

#[test]
fn a_copy_of_a_build_shares_its_parses_and_owns_its_graph() {
    let mut build = GraphBuild::of(
        [
            parse("behavior a \"A\" {\n}\n", "a.spec"),
            parse("behavior b \"B\" {\n}\n", "b.spec"),
        ],
        GraphConfig::default(),
    );
    let copy = build.clone();
    let (_, mine) = build.files().next().unwrap();
    let (_, theirs) = copy.files().next().unwrap();
    assert!(std::ptr::eq(mine, theirs), "the parses are shared");
    let before = copy.diagnostics().to_vec();

    build.apply([FileChange::Removed("a.spec".into())]);

    assert!(copy.graph().node("a").is_some(), "the copy still holds a");
    assert!(build.graph().node("a").is_none());
    assert_eq!(copy.diagnostics(), before.as_slice());
}
