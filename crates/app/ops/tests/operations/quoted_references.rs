//! A quoted reference is a reference (ADR 0093 D3) written at its ID, inside its quotes (ADR 0218): rename, the
//! navigator's tokens and references, the token at a position and the replace fix read it there.

use crate::fixes::{apply, fixes_of};
use crate::navigate::{Compiled, SOFTWARE, at, compile};
use specforge_ops::authoring::fix::{FixQuery, FixSource};
use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::{Direction, Precision, ReferenceQuery};
use specforge_test_macros::test as specforge_test;

const INCOMING: ReferenceQuery = ReferenceQuery {
    direction: Direction::Incoming,
    include_declaration: false,
};

/// `payload "user"` (5:12-5:16 inside its quotes), `produces ["ev_one", ev_two]` (10:14-10:20), and an unresolved
/// `payload "usr"` (14:12-14:15) whose close match is `user`.
const QUOTED: &str = "type user \"User\" {\n}\n\n\
                      event ev_one \"E one\" {\n  payload \"user\"\n}\n\n\
                      behavior pay \"Pay\" {\n  contract \"c\"\n  produces [\"ev_one\", ev_two]\n}\n\n\
                      event ev_two \"E two\" {\n  payload \"usr\"\n}\n";

fn quoted() -> Compiled {
    compile(SOFTWARE, &[("main.spec", QUOTED)])
}

#[specforge_test(
    behavior = "rename_entity_id",
    verify = "rename edits a quoted reference inside its quotes"
)]
fn rename_edits_a_quoted_reference_inside_its_quotes() {
    let p = quoted();
    let plan = rename::plan(&p.navigator(), "user", "account").unwrap();
    let mut spans: Vec<String> = plan.edits().iter().map(|e| at(&e.span)).collect();
    spans.sort();
    assert_eq!(spans, ["main.spec 1:6-1:10", "main.spec 5:12-5:16"]);
    let text = apply(QUOTED, plan.edits());
    assert!(text.contains("type account \"User\""), "{text}");
    assert!(text.contains("payload \"account\""), "{text}");

    let plan = rename::plan(&p.navigator(), "ev_one", "ev_first").unwrap();
    let text = apply(QUOTED, plan.edits());
    assert!(text.contains("produces [\"ev_first\", ev_two]"), "{text}");
}

#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "find_references answers a quoted reference at its ID, token-precise"
)]
fn references_answer_a_quoted_reference_at_its_id() {
    let p = quoted();
    let nav = p.navigator();
    let shown: Vec<_> = nav
        .references("user", INCOMING)
        .unwrap()
        .iter()
        .map(|o| (at(&o.span), o.precision))
        .collect();
    assert_eq!(
        shown,
        [("main.spec 5:12-5:16".to_string(), Precision::Token)]
    );
    let shown: Vec<_> = nav
        .tokens("ev_one", false)
        .unwrap()
        .iter()
        .map(|t| (at(&t.span), t.precision))
        .collect();
    assert_eq!(
        shown,
        [("main.spec 10:14-10:20".to_string(), Precision::Token)]
    );
}

#[specforge_test(
    behavior = "prepare_rename",
    verify = "prepare rename answers on a quoted reference, inside its quotes"
)]
fn the_token_at_a_quoted_reference_is_its_id() {
    let p = quoted();
    let nav = p.navigator();
    let token = nav.occurrence_at("main.spec", 5, 13).expect("a token");
    assert_eq!(token.target.as_str(), "user");
    assert_eq!(at(&token.span), "main.spec 5:12-5:16");
    // The opening quote is not the ID.
    assert!(nav.occurrence_at("main.spec", 5, 11).is_none());
}

#[specforge_test(
    behavior = "code_action_replace_unresolved",
    verify = "a quoted unresolved reference with a close match is replaced inside its quotes"
)]
fn a_quoted_unresolved_reference_is_replaced_inside_its_quotes() {
    let p = quoted();
    let query = FixQuery {
        code: Some("E003"),
        ..FixQuery::default()
    };
    let fix = fixes_of(&p, &query)
        .into_iter()
        .find(|f| f.source == FixSource::ReplaceUnresolved)
        .expect("a replace fix");
    assert_eq!(fix.title, "Replace with 'user'");
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(at(&fix.edits[0].span), "main.spec 14:12-14:15");
    assert_eq!(
        apply(QUOTED, &fix.edits),
        QUOTED.replace("payload \"usr\"", "payload \"user\"")
    );
}
