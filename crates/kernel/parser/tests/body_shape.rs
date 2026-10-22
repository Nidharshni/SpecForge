//! A block answers its body shape from its syntax: a union or braced, whatever its fields are named (ADR 0219).

use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "a block answers whether it is a union from its syntax, whatever its fields are named"
)]
fn a_block_answers_whether_it_is_a_union() {
    let file = parse(
        "type Status = open | done\n\ntype Feature \"F\" {\n  variants string[]\n}\n\n\
         type Pick \"P\" {\n  variants a | b\n}\n\nport gate \"G\" = open | shut\n\n\
         type Broken \"B\" =\n",
        "t.spec",
    );
    let shapes: Vec<(&str, bool)> = file
        .entities
        .iter()
        .map(|e| (e.id.raw.as_str(), e.fields.is_union()))
        .collect();
    assert_eq!(
        shapes,
        [
            ("Status", true),
            ("Feature", false),
            ("Pick", false),
            ("gate", true),
            ("Broken", true)
        ]
    );
}
