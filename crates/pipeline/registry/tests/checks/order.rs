//! The order the checks run in: one fixed sequence behind one gate, the
//! same on every surface. Written as `.spec` text (ADR 0115).

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_test_macros::test as spec;

use crate::support::written::checked;
use crate::support::{build, declare};

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
            k.field("note", |f| {
                f.field_type(FieldType::String);
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
    verify = "the structural checks run in one order: W012, E024, E013, E014, W020, W158, E061, W159, then the rule set, the host's E006 and E016 rules last"
)]
fn the_checks_run_in_one_order() {
    // One entity per check, written in an order that is not the checks'.
    let text = "ref gh.issue:1 \"R\"\n\n\
                gadget g_docs \"D\" {\n  docs [\"no_such_file_04.md\"]\n}\n\n\
                gadget g_size \"S\" {\n  size \"big\"\n}\n\n\
                widget w_one \"W\" {\n}\n\n\
                gadget g_bogus \"B\" {\n  bogus \"\"\n  verify unit \"x\"\n}\n\n\
                gadget g_note \"N\" {\n  note bare\n}\n\n\
                gadget x \"X\" {\n}\n\n\
                gadget gadget \"G\" {\n}\n\n\
                wibble wb_one \"Wb\" {\n}\n";

    let diags = checked(&build([shapes()]), &[("a.spec", text)]);

    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        [
            "W012", "E024", "E013", "E014", "W020", "W158", "E061", "W159", "W900", "E016"
        ],
        "{diags:?}"
    );
}
