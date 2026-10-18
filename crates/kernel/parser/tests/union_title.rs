//! A union block takes a title like every other block (ADR 0148).

use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "a union block carries its title"
)]
fn a_union_block_carries_its_title() {
    let file = parse("type S \"States\" = a | b\n", "t.spec");
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    assert_eq!(file.entities.len(), 1);
    assert_eq!(file.entities[0].title.as_deref(), Some("States"));
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "a union block carries its title"
)]
fn a_union_title_is_unescaped() {
    let file = parse("type S \"S \\\"q\\\"\" = a | b\n", "t.spec");
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    assert_eq!(file.entities[0].title.as_deref(), Some("S \"q\""));
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "a union block carries its title"
)]
fn a_union_without_a_title_still_parses() {
    let file = parse("type S = a | b\n", "t.spec");
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    assert_eq!(file.entities[0].title, None);
}
