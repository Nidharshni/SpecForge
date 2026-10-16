//! What the registry build derives for the graph build: `graph_inputs()`.

use std::collections::HashMap;

use specforge_extension_sdk::prelude::*;
use specforge_graph::{DerivedFrom, FieldForm, FieldForms, GraphInputs, Reading, Target};
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
                    if field_type == FieldType::Enum {
                        f.vocabulary(&Vocabulary::new(&["x", "y"]));
                    } else {
                        f.field_type(field_type);
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
fn a_fields_form_follows_its_registered_type() {
    let build = build([typed_fields()]);
    let inputs = build.graph_inputs();

    let expected: HashMap<(String, String), FieldForm> = [
        ("a_string", FieldType::String),
        ("an_integer", FieldType::Integer),
        ("a_bool", FieldType::Bool),
        ("an_enum", FieldType::Enum),
        ("a_string_list", FieldType::StringList),
        ("a_reference", FieldType::Reference),
        ("a_reference_list", FieldType::ReferenceList),
        ("a_block", FieldType::Block),
    ]
    .into_iter()
    .map(|(field, field_type)| {
        let values = if field_type == FieldType::Enum {
            vec!["x".to_string(), "y".to_string()]
        } else {
            Vec::new()
        };
        (
            ("thing".to_string(), field.to_string()),
            FieldForm::new(field_type, values),
        )
    })
    .collect();
    // Every registered field has a form; a single reference and a block are
    // stored as written (their form coerces nothing).
    assert_eq!(
        inputs.field_forms,
        FieldForms::new(
            expected,
            Default::default(),
            ["thing".to_string()].into_iter().collect()
        )
    );
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
    let build = build([declaration]);
    let inputs = build.graph_inputs();
    let derived = &inputs.derived_references;

    let summary: Vec<(&str, &str, &str, DerivedFrom)> = derived
        .iter()
        .map(|d| {
            (
                d.source_kind.as_str(),
                d.field.as_str(),
                inputs
                    .field_forms
                    .target(&d.source_kind, &d.field)
                    .unwrap()
                    .kind(),
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

/// `kind` with the field `name` to `target`, inverse of `inverse`.
fn inverse_pair(
    extension: &str,
    kind: &str,
    name: &str,
    target: &str,
    inverse: &str,
) -> ExtensionDeclaration {
    declare(extension, |c| {
        c.kind(kind, |k| {
            k.keyword(kind);
            k.field(name, |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind(target)
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
            inverse_pair("@t/z", "zed", "z", "em", "m"),
            inverse_pair("@t/b", "bee", "b_field", "ay", "a_field"),
            inverse_pair("@t/a", "ay", "a_field", "bee", "b_field"),
            inverse_pair("@t/m", "em", "m", "zed", "z"),
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
    verify = "an inverse field is a field of the target kind: the one inverse_of names, and every one naming the field inverse_of"
)]
fn inverse_fields_are_fields_of_the_target_kind() {
    let declared = declare("@t/tree", |c| {
        c.kind("Parent", |k| {
            k.keyword("parent");
            // Names `parent_ref`, which child registers.
            k.field("children", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("child")
                    .inverse_of("parent_ref");
            });
            // Names `nope`, which child does not register.
            k.field("dangling", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("child")
                    .inverse_of("nope");
            });
            k.field("no_target", |f| {
                f.field_type(FieldType::ReferenceList).inverse_of("nope");
            });
        });
        c.kind("Child", |k| {
            k.keyword("child");
            k.field("parent_ref", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("parent");
            });
            // Names `children`; and `parent_ref` is named by it too.
            k.field("other", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("parent")
                    .inverse_of("children");
            });
        });
    });
    let build = build([declared]);
    let fields = build.fields();

    // The field `inverse_of` names, and every field of the target kind
    // naming this one: `children` names `parent_ref` and `other` names it.
    assert_eq!(
        fields.inverse_fields("parent", "children"),
        ["other", "parent_ref"]
    );
    // `parent_ref` names none; `children` names it.
    assert_eq!(fields.inverse_fields("child", "parent_ref"), ["children"]);
    // `other` names `children`, and nothing else names `other`.
    assert_eq!(fields.inverse_fields("child", "other"), ["children"]);
    // An inverse_of the target kind lacks is no inverse; nor is a field
    // with no target kind, or one that is not registered.
    assert!(fields.inverse_fields("parent", "dangling").is_empty());
    assert!(fields.inverse_fields("parent", "no_target").is_empty());
    assert!(fields.inverse_fields("parent", "missing").is_empty());
    assert!(fields.inverse_fields("nobody", "children").is_empty());

    // The graph's pairs are the same relation: no pair for `dangling`.
    assert_eq!(
        build.graph_inputs().bidirectional_pairs,
        [
            ("children".to_string(), "other".to_string()),
            ("children".to_string(), "parent_ref".to_string())
        ]
    );
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
    assert!(
        inputs
            .field_forms
            .reading("doc", "owner")
            .is_single_reference()
    );
    assert_eq!(
        inputs.field_forms.target("doc", "owner"),
        Some(Target::Absent("team"))
    );
    assert_eq!(
        inputs.bidirectional_pairs,
        [("part_of".to_string(), "parts".to_string())]
    );
    assert_eq!(inputs.derived_references.len(), 1);
    assert_eq!(
        match inputs.field_forms.reading("thing", "an_integer") {
            Reading::Declared(form) => Some(form.field_type()),
            _ => None,
        },
        Some(FieldType::Integer)
    );
    // deterministic: the same declarations give the same inputs.
    assert_eq!(crate::support::build(declared()).graph_inputs(), inputs);
    assert_ne!(inputs, &GraphInputs::default());
}

#[spec(
    behavior = "registry_build_graph_inputs",
    verify = "a reference field's form says whether its target kind is loaded"
)]
fn the_field_forms_say_whether_each_target_kind_is_loaded() {
    let forms = build([declare("@t/ext", |c| {
        c.kind("Doc", |k| {
            k.keyword("doc");
            k.field("owner", |f| {
                f.field_type(FieldType::Reference).target_kind("team");
            });
            k.field("parts", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("doc");
            });
            k.field("title", |f| {
                f.field_type(FieldType::String);
            });
        });
    })])
    .graph_inputs()
    .field_forms
    .clone();
    assert_eq!(forms.target("doc", "owner"), Some(Target::Absent("team")));
    assert_eq!(forms.target("doc", "parts"), Some(Target::Loaded("doc")));
    assert_eq!(forms.target("doc", "title"), None);
}

#[spec(
    behavior = "registry_build_graph_inputs",
    verify = "the graph inputs name the extension that owns each absent target kind"
)]
fn the_graph_inputs_name_each_absent_target_kinds_owner() {
    let product = declare("@t/product", |c| {
        c.kind("Feature", |k| {
            k.keyword("feature");
        });
    });
    let software = declare("@t/software", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            k.field("features", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("feature");
            });
            k.field("owner", |f| {
                f.field_type(FieldType::Reference).target_kind("team");
            });
            k.field("peers", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("behavior");
            });
        });
    });
    let vocabulary = crate::support::candidates(&[product]);

    // A build never handed a vocabulary names no owner.
    let unhanded = build([software]);
    assert!(unhanded.graph_inputs().target_owners.is_empty());

    // Handing it one derives the inputs again, with no order to call them in.
    let handed = unhanded.with_candidates(vocabulary);
    let inputs = handed.graph_inputs();
    assert_eq!(inputs.target_owners.len(), 1, "{:?}", inputs.target_owners);
    assert_eq!(inputs.target_owners["feature"], "@t/product");
    // `team` is owned by no one; `behavior` is loaded.
    assert!(!inputs.target_owners.contains_key("team"));
    assert!(!inputs.target_owners.contains_key("behavior"));
}
