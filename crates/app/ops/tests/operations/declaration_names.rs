//! Where a declaration's name is written: where the parse read it (ADR 0187).

use crate::navigate::{SOFTWARE, at, compile};
use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::{Precision, outline};
use specforge_test::prelude::*;

const BELOW: &str = "invariant\n  session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

#[specforge_test(
    behavior = "go_to_definition",
    verify = "the definition selects a name written below its kind"
)]
fn the_definition_of_a_name_written_below_its_kind_selects_the_name() {
    let p = compile(SOFTWARE, &[("limit.spec", BELOW), ("login.spec", LOGIN)]);
    let nav = p.navigator();
    let definition = nav.definition("session_limit").unwrap();
    assert_eq!(at(&definition.name), "limit.spec 2:3-2:16");
    assert_eq!(definition.precision, Precision::Token);
    let outline = outline(&nav, "limit.spec").unwrap();
    assert_eq!(at(&outline[0].name), "limit.spec 2:3-2:16");
    let token = nav.occurrence_at("limit.spec", 2, 5).unwrap();
    assert_eq!(token.target, "session_limit");
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "a rename edits a declaration whose name is written below its kind"
)]
fn a_rename_of_a_name_written_below_its_kind_edits_the_declaration() {
    let p = compile(SOFTWARE, &[("limit.spec", BELOW), ("login.spec", LOGIN)]);
    let plan = rename::plan(&p.navigator(), "session_limit", "session_cap").unwrap();
    let mut spans: Vec<String> = plan.edits().iter().map(|e| at(&e.span)).collect();
    spans.sort();
    assert_eq!(spans, ["limit.spec 2:3-2:16", "login.spec 2:15-2:28"]);
}

#[test]
fn a_spec_blocks_definition_is_its_block() {
    let p = compile(SOFTWARE, &[("guide.spec", "spec \"Guide\" {\n}\n")]);
    let nav = p.navigator();
    let definition = nav.definition("Guide").unwrap();
    assert_eq!(definition.name, definition.block);
    assert_eq!(definition.precision, Precision::Entity);
    assert!(nav.occurrence_at("guide.spec", 1, 8).is_none());
}
