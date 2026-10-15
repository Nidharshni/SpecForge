//! The hover lists an entity's `verify` statements apart from its fields (ADR 0094).

use crate::hover::coverage_project;

#[test]
#[ignore = "flipped by T7"]
fn hover_lists_obligations_apart_from_fields() {
    let project = coverage_project(
        "type Payload \"P\" {\n  verify string @optional\n  verify unit \"payload is valid\"\n}\n",
    );
    let hover = project.hover_on("main.spec", "Payload").unwrap();
    assert!(hover.contains("- `verify` = `string`"), "{hover}");
    assert!(
        hover.contains("**Obligations**\n- unit: payload is valid"),
        "{hover}"
    );
    assert!(!hover.contains("`verify` = unit"), "{hover}");
}
