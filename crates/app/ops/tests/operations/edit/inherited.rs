//! What a typed edit reads of a field its block inherits (ADR 0149, ADR 0175, ADR 0182): a single reference the block
//! takes from its file's defaults, and a list default written as one value.

use super::*;

/// `payload` (a `reference`) and `refs` (a `string_list` written as one value) come from the defaults; the event
/// writes neither.
const EVENTS: &str = "defaults event {\n  payload Order\n  refs    \"a\"\n}\n\ntype Order \"Order\" {\n  id string\n}\n\n\
event order_placed \"Order Placed\" {\n  contract \"The system MUST publish it\"\n}\n";

fn events() -> Compiled {
    project(&[("e.spec", EVENTS)])
}

#[test]
#[ignore = "flipped by T6"]
fn unlink_of_an_inherited_single_reference_is_refused_as_unset_is() {
    let c = events();
    let unlinked = refused(&c, &[unlink("order_placed", "payload", "Order")]);
    assert_eq!(unlinked.0, "not_editable", "{}", unlinked.1);
    assert_eq!(unlinked, refused(&c, &[unset("order_placed", "payload")]));
}

#[test]
#[ignore = "flipped by T7"]
fn set_to_a_one_value_default_leaves_the_block_inheriting_it() {
    let c = events();
    assert!(
        plan(&c, &[set("order_placed", "refs", one("a"))])
            .unwrap()
            .is_empty()
    );
    assert!(
        plan(&c, &[set("order_placed", "refs", many(&["a"]))])
            .unwrap()
            .is_empty()
    );
    // A create is one set per field, so `new` follows `set` here.
    let new = after(
        &c,
        "e.spec",
        &[create(
            "event",
            "order_shipped",
            "e.spec",
            &[("refs", one("a")), ("contract", one("c"))],
        )],
    );
    assert!(!new.contains("[\"a\"]"), "{new}");
}

#[test]
#[ignore = "flipped by T7"]
fn a_link_and_its_unlink_over_a_one_value_default_leave_the_text_as_it_was() {
    let c = events();
    let planned = plan(
        &c,
        &[
            link("order_placed", "refs", "b"),
            unlink("order_placed", "refs", "b"),
        ],
    )
    .unwrap();
    assert!(planned.is_empty(), "{:?}", planned.after());
}
