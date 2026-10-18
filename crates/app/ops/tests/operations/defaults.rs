//! File defaults (ADR 0149) through navigation and rename: an inherited
//! reference is one token, held by every block that inherits it.

use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::Role;

use crate::navigate::{SOFTWARE, compile};

const BLOCKS: &str = "defaults behavior {\n  ports [Repo]\n}\n\n\
                      behavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\nbehavior c \"C\" {\n}\n";
const PORT: &str = "port Repo \"Repo\" {\n}\n";

#[test]
#[ignore = "flipped by T4"]
fn the_occurrence_at_a_defaults_token_is_its_reference() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let occurrence = p.navigator().occurrence_at("a.spec", 2, 10).unwrap();
    assert_eq!(occurrence.role, Role::Reference);
    assert_eq!(occurrence.target, "Repo");
}

#[test]
#[ignore = "flipped by T4"]
fn renaming_an_inherited_reference_edits_it_once() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let plan = rename::plan(&p.navigator(), "Repo", "Store").unwrap();
    let on_line = plan
        .edits
        .iter()
        .filter(|e| e.file == "a.spec" && e.line == 2)
        .count();
    assert_eq!(on_line, 1, "{:?}", plan.edits);
}
