//! What the registry build derives for the graph build: `graph_inputs()`.

use std::collections::HashMap;

use specforge_extension_sdk::prelude::*;
use specforge_graph::{DerivedFrom, FieldCoercion, GraphInputs};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_test_macros::test as spec;

use crate::support::{build, declare};

/// A kind `thing` with one field per type.
fn typed_fields() -> ExtensionDeclaration {
    declare("@t/typed", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing");
            for (name, field_type) in [
                ("a_string", FieldType::String),
                ("an_integer", FieldType::Integer),
                ("a_bool", FieldType::Bool),
                ("an_enum", FieldType::Enum),
                ("a_string_list", FieldType::StringList),
                ("a_reference", FieldType::Reference),
                ("a_reference_list", FieldType::ReferenceList),
                ("a_block", FieldType::Block),
            ] {
                k.field(name, |f| {
                    f.field_type(field_type);
                    if field_type == FieldType::Enum {
                        f.enum_values(&["x", "y"]);
                    }
                });
            }
        });
    })
}

#[spec(
    behavior = "registry_build_graph_inputs",
    verify = "a field's value coercion follows its registered type"
)]
fn a_fields_coercion_follows_its_registered_type() {
    let inputs = build([typed_fields()]).graph_inputs();

    let expected: HashMap<(String, String), FieldCoercion> = [
        ("a_string", FieldCoercion::Text),
        ("an_integer", FieldCoercion::Integer),
        ("a_bool", FieldCoercion::Bool),
        ("an_enum", FieldCoercion::Text),
        ("a_string_list", FieldCoercion::List),
        ("a_reference_list", FieldCoercion::List),
    ]
    .into_iter()
    .map(|(field, coercion)| (("thing".to_string(), field.to_string()), coercion))
    .collect();
    // A single reference and a block are read as written.
    assert_eq!(inputs.field_coercions, expected);
}

#[spec(
    behavior = "registry_build_graph_inputs",
    verify = "a derived reference field is a graph input only with a target kind and a known source"
)]
fn a_derived_reference_needs_a_target_kind_and_a_known_source() {
    let declaration = declare("@t/derived", |c| {
        c.kind("Shape", |k| {
            k.keyword("shape");
            k.field("uses_types", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("type_expressions");
            });
            k.field("calls", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("method_signatures");
            });
            k.field("no_target", |f| {
                f.field_type(FieldType::ReferenceList)
                    .derived_from("type_expressions");
            });
            k.field("unknown_source", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("carrier_pigeons");
            });
        });
    });
    let derived = build([declaration]).graph_inputs().derived_references;

    let summary: Vec<(&str, &str, &str, DerivedFrom)> = derived
        .iter()
        .map(|d| {
            (
                d.source_kind.as_str(),
                d.field.as_str(),
                d.target_kind.as_str(),
                d.from,
            )
        })
        .collect();
    // Sorted by (kind, field).
    assert_eq!(
        summary,
        [
            ("shape", "calls", "shape", DerivedFrom::MethodSignatures),
            ("shape", "uses_types", "shape", DerivedFrom::TypeExpressions),
        ]
    );
}

/// `kind` with the field `name` inverse of `inverse`.
fn inverse_pair(extension: &str, kind: &str, name: &str, inverse: &str) -> ExtensionDeclaration {
    declare(extension, |c| {
        c.kind(kind, |k| {
            k.keyword(kind);
            k.field(name, |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind(kind)
                    .inverse_of(inverse);
            });
        });
    })
}

#[spec(
    behavior = "registry_build_graph_inputs",
    verify = "inverse pairs are written lesser first and sorted, whatever the registration order"
)]
fn inverse_pairs_are_written_lesser_first_and_sorted() {
    let declared = || {
        [
            inverse_pair("@t/z", "zed", "z", "m"),
            inverse_pair("@t/b", "bee", "b_field", "a_field"),
            inverse_pair("@t/a", "ay", "a_field", "b_field"),
            inverse_pair("@t/m", "em", "m", "z"),
        ]
    };
    let expected = vec![
        ("a_field".to_string(), "b_field".to_string()),
        ("m".to_string(), "z".to_string()),
    ];

    assert_eq!(
        build(declared()).graph_inputs().bidirectional_pairs,
        expected
    );
    let mut reversed: Vec<_> = declared().into_iter().collect();
    reversed.reverse();
    assert_eq!(build(reversed).graph_inputs().bidirectional_pairs, expected);
}

#[spec(
    behavior = "registry_build_graph_inputs",
    verify = "Registry Build Derives the Graph Inputs: graph inputs hold — registries_populated, every_input_derived, registries_agree, deterministic"
)]
fn the_graph_inputs_contract_holds() {
    let declared = || {
        [
            declare("@t/ext", |c| {
                c.kind("Doc", |k| {
                    k.keyword("doc").has_body_parser();
                    k.field("owner", |f| {
                        f.field_type(FieldType::Reference).target_kind("team");
                    });
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
                    k.field("sees", |f| {
                        f.field_type(FieldType::ReferenceList)
                            .target_kind("doc")
                            .derived_from("type_expressions");
                    });
                });
            }),
            typed_fields(),
        ]
    };
    let build = build(declared());
    let inputs = build.graph_inputs();

    // registries_populated, registries_agree: each input says what the build holds.
    assert_eq!(
        inputs.body_parser_kinds,
        ["doc".to_string()].into_iter().collect()
    );
    assert!(
        inputs
            .single_reference_fields
            .contains(&("doc".to_string(), "owner".to_string()))
    );
    assert_eq!(
        inputs
            .absent_reference_targets
            .get(&("doc".to_string(), "owner".to_string()))
            .map(String::as_str),
        Some("team")
    );
    assert_eq!(
        inputs.bidirectional_pairs,
        [("part_of".to_string(), "parts".to_string())]
    );
    assert_eq!(inputs.derived_references.len(), 1);
    assert_eq!(
        inputs
            .field_coercions
            .get(&("thing".to_string(), "an_integer".to_string())),
        Some(&FieldCoercion::Integer)
    );
    // deterministic: the same declarations give the same inputs.
    assert_eq!(crate::support::build(declared()).graph_inputs(), inputs);
    assert_ne!(inputs, GraphInputs::default());
}
