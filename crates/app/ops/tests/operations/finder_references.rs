//! Every "where is this name written" question is the graph's finder's (ADR 0176): the token at a position and
//! find-references agree with rename on a default no block takes and on a duplicate declaration.

use specforge_ops::read::navigate::ReferenceQuery;

use crate::navigate::{SOFTWARE, at, compile};

const UNTAKEN: &str = "defaults behavior {\n  ports [Repo]\n}\n\nport Repo \"Repo\" {\n}\n";
const DUPLICATE: &str = "behavior a \"A\" {\n  ports [Repo]\n}\n\nbehavior a \"Again\" {\n  ports [Repo]\n}\n\n\
                         port Repo \"Repo\" {\n  direction inbound\n}\n";

#[test]
fn the_token_at_a_default_no_block_takes_names_its_entity() {
    let p = compile(SOFTWARE, &[("main.spec", UNTAKEN)]);
    let token = p.navigator().occurrence_at("main.spec", 2, 10).unwrap();
    assert_eq!(token.target, "Repo");
    assert_eq!(at(&token.span), "main.spec 2:10-2:14");
}

#[test]
fn the_token_at_a_reference_in_a_duplicate_declaration_names_its_entity() {
    let p = compile(SOFTWARE, &[("main.spec", DUPLICATE)]);
    let token = p.navigator().occurrence_at("main.spec", 6, 10).unwrap();
    assert_eq!(token.target, "Repo");
    assert_eq!(at(&token.span), "main.spec 6:10-6:14");
}

#[test]
#[ignore = "flipped by T5"]
fn find_references_lists_a_reference_in_a_duplicate_declaration() {
    let p = compile(SOFTWARE, &[("main.spec", DUPLICATE)]);
    let found = p
        .navigator()
        .references("Repo", ReferenceQuery::default())
        .unwrap();
    let shown: Vec<String> = found
        .iter()
        .map(|o| format!("{} {}", o.holder, at(&o.span)))
        .collect();
    assert_eq!(shown, ["a main.spec 2:10-2:14", "a main.spec 6:10-6:14"]);
}
