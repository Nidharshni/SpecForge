//! The order the checks run in: one fixed sequence behind one gate, the
//! same on every surface.

use specforge_extension_sdk::prelude::*;
use specforge_graph::ValueShape;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use crate::support::{build, check, declare, span};

/// `gadget` (an integer `size`, a reference list `parts` to gadgets),
/// `widget`, and a rule that fires on a widget that links nothing.
fn shapes() -> ExtensionDeclaration {
    declare("@test/shapes", |c| {
        c.kind("Gadget", |k| {
            k.keyword("gadget");
            k.field("size", |f| {
                f.field_type(FieldType::Integer);
            });
            k.field("parts", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("gadget");
            });
            k.field("docs", |f| {
                f.field_type(FieldType::StringList).file_reference();
            });
        });
        c.kind("Widget", |k| {
            k.keyword("widget");
        });
        c.rule("W900", |r| {
            r.check(CheckKind::NoOutgoingEdges)
                .severity(ValidationSeverity::Warning)
                .target_kind("widget")
                .message_template("{kind} '{id}' links nothing");
        });
    })
}

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "the structural checks run in one order: W012, E016, E024, E013, E014, W020, E022, E061, then the rule set"
)]
fn the_checks_run_in_one_order() {
    let s = span("a.spec");
    // One record per check, written in an order that is not the checks'.
    let records = vec![
        EntityRecord::new("ref", "gh.issue:1", s),
        EntityRecord::new("gadget", "g_docs", s).with_list("docs", &["no_such_file_04.md"]),
        EntityRecord::new("gadget", "g_size", s).with_value("size", ValueShape::String, "big"),
        EntityRecord::new("gadget", "g_parts", s).with_reference("parts", &["w_one"]),
        EntityRecord::new("widget", "w_one", s),
        EntityRecord::new("gadget", "g_bogus", s).with_fields(&["bogus"]),
        EntityRecord::new("gadget", "x", s),
        EntityRecord::new("gadget", "gadget", s),
        EntityRecord::new("wibble", "wb_one", s),
    ];

    let diags = check(&build([shapes()]), &records);

    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        [
            "W012", "E016", "E024", "E013", "E014", "W020", "E022", "E061", "W900"
        ],
        "{diags:?}"
    );
}
