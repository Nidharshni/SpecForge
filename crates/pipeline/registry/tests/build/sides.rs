//! W162: a relation declared from both of its ends (ADR 0150).

use specforge_extension_sdk::prelude::*;

use crate::support::{build, coded, codes, declare};

#[test]
#[ignore = "flipped by T10"]
fn an_inverse_of_naming_a_field_of_the_target_kind_is_w162() {
    let declared = declare("@t/docs", |c| {
        c.kind("Doc", |k| {
            k.keyword("doc");
            k.field("parts", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("doc")
                    .inverse_of("part_of");
            });
            k.field("part_of", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("doc")
                    .inverse_of("parts");
            });
        });
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            k.field("features", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("feature");
            });
        });
        c.kind("Milestone", |k| {
            k.keyword("milestone");
            k.field("behaviors", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("behavior")
                    .inverse_of("features");
            });
        });
    });
    let build = build([declared]);
    assert_eq!(coded(&build, "W162").len(), 3, "{:?}", codes(&build));
}

#[test]
#[ignore = "flipped by T10"]
fn two_kinds_linked_both_ways_are_w162() {
    let declared = declare("@t/tree", |c| {
        c.kind("Parent", |k| {
            k.keyword("parent");
            k.field("children", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("child");
            });
        });
        c.kind("Child", |k| {
            k.keyword("child");
            k.field("parent", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("parent");
            });
        });
    });
    let build = build([declared]);
    let found = coded(&build, "W162");
    assert_eq!(found.len(), 1, "{:?}", codes(&build));
    assert!(
        found[0].message.contains("parent.children") && found[0].message.contains("child.parent")
    );
}
