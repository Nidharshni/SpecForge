//! A union block takes a title like every other block (ADR 0148).

use specforge_parser::parse;

#[test]
#[ignore = "flipped by T3"]
fn a_union_block_carries_its_title() {
    let file = parse("type S \"States\" = a | b\n", "t.spec");
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    assert_eq!(file.entities.len(), 1);
    assert_eq!(file.entities[0].title.as_deref(), Some("States"));
}
