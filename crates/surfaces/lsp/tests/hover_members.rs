//! The hover lists an entity's methods and each field's annotations (ADR 0222).

use specforge_test_macros::test as spec;

use crate::registries::kind;
use crate::served::Served;

const TEXT: &str = r#"type Order "Order" {
  id string @optional @readonly
}
port order_store "Order store" {
  method save(order: Order @optional, force?: bool) -> Result<void, never>
  verify unit "saves orders"
}
"#;

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover lists the entity's methods and each field's annotations"
)]
fn hover_lists_methods_and_annotations() {
    let project = Served::new(&[("main.spec", TEXT)])
        .extension("@t/soft", |c| {
            kind(c, "type", false);
            c.kind("port", |k| {
                k.testable(true).supports_verify(true);
            });
        })
        .open(&["main.spec"]);
    let port = project.hover_on("main.spec", "order_store").unwrap();
    assert!(
        port.contains(
            "**Methods**\n- `save(order: Order @optional, force?: bool) -> Result<void, never>`"
        ),
        "{port}"
    );
    let order = project.hover_on("main.spec", "Order").unwrap();
    assert!(
        order.contains("- `id` = `string` @optional @readonly"),
        "{order}"
    );
}
