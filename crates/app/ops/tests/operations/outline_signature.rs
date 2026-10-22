//! The outline's signature is the method as written (ADR 0222): a parameter's annotations included.

use crate::navigate::{SOFTWARE, compile};
use specforge_ops::read::navigate::outline;
use specforge_test::prelude::*;

#[specforge_test(
    behavior = "outline_view",
    verify = "an outline method's signature is the method as written, its parameters' annotations included"
)]
#[ignore = "flipped by T3"]
fn an_outline_signature_carries_a_parameters_annotations() {
    let p = compile(
        SOFTWARE,
        &[(
            "store.spec",
            "type Order \"Order\" {\n  id string\n}\nport store \"Store\" {\n  direction outbound\n  method save(order: Order @optional, force?: bool) -> Result<void, never>\n}\n",
        )],
    );
    let entries = outline(&p.navigator(), "store.spec").expect("a source");
    let save = &entries[1].children[0];
    assert_eq!(
        save.signature,
        "save(order: Order @optional, force?: bool) -> Result<void, never>"
    );
}
